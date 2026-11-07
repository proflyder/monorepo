//! Deployment: build the payload in a checkout, copy it to `/data/pulsar`, write the boot
//! hook, migrate old `rootkeeper.sh` hooks and verify the restarted daemon.
//!
//! The command order and every message are frozen by `testdata/deploy_*.json` generated
//! from the Go installer (see docs/development.md, «Правила installer/»).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::adb::Adb;
use crate::probe::{arch_of, has_zigbee, model_name, station_code};
use crate::scripts::{
    HOOK_SAT, INIT_PATH, check_rootkeeper, init_script, root_mount_rw, rootfs_guard, split_lines,
    unhook_rootkeeper, write_init_cmd,
};

pub const DATA_DIR: &str = "/data/pulsar";
pub const RK_PATH: &str = "/data/quasar/rootkeeper.sh";

pub struct Ui<'a> {
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
}

impl Ui<'_> {
    fn line(&mut self, s: &str) {
        let _ = writeln!(self.out, "{s}");
    }

    fn blank(&mut self) {
        let _ = writeln!(self.out);
    }

    fn error(&mut self, s: &str) {
        let _ = writeln!(self.err, "{s}");
    }
}

pub fn sha256_file(p: &Path) -> String {
    let Ok(mut f) = std::fs::File::open(p) else {
        return String::new();
    };
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        match std::io::Read::read(&mut f, &mut buf) {
            Ok(0) => break,
            Ok(n) => h.update(&buf[..n]),
            Err(_) => return String::new(),
        }
    }
    hex::encode(h.finalize())
}

/// The text up to the first space (sha256sum output -> digest).
pub fn first_field(s: &str) -> &str {
    s.split(' ').next().unwrap_or("")
}

fn sha256_str(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

fn exit_status_text(s: &std::process::ExitStatus) -> String {
    s.code().map_or_else(
        || "terminated by signal".to_string(),
        |c| format!("exit status {c}"),
    )
}

fn git_repo_root(root: &Path) -> PathBuf {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if text.is_empty() {
                root.to_path_buf()
            } else {
                PathBuf::from(text)
            }
        }
        _ => root.to_path_buf(),
    }
}

/// `filepath.Ext`: the suffix from the last dot, including a name that is only ".tflite".
fn go_ext(name: &str) -> &str {
    name.rfind('.').map_or("", |i| &name[i..])
}

/// Builds missing outputs in a repository checkout; the release archive ships them and no
/// sources.
///
/// Deliberate difference from the Go installer: it called `bazel` from the payload directory
/// and read `<payload>/bazel-bin`, which only worked when the checkout root was the pulsar
/// directory. This version accepts `bazel` or the repository's `bazelisk`, runs it at the Git
/// top level with `--disk_cache=` and asks `bazel info` for the real output base.
pub fn build(root: &Path, ssh: bool, ui: &mut Ui) -> bool {
    let sat = root.join("satellite");
    if !sat.join("BUILD.bazel").is_file() {
        return true;
    }
    ui.line("== build");
    let fail = |ui: &mut Ui, e: &dyn std::fmt::Display| {
        ui.error(&format!(
            "build: {e} (building from source needs Linux or WSL with Bazelisk, see README)"
        ));
        false
    };
    let mut version = String::from("unknown");
    if let Ok(b) = std::fs::read_to_string(root.join("VERSION")) {
        let v = b.trim();
        if !v.is_empty() {
            version = v.to_string();
        }
    }
    let repo = git_repo_root(root);
    let Some(bazel) = crate::platformtools::bazel_tool() else {
        return fail(ui, &"bazel or bazelisk not found in PATH");
    };
    let mut build_time = String::from("unknown");
    let git = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "log",
            "-1",
            "--format=%cd",
            "--date=format-local:%b %e %Y, %H:%M:%S",
        ])
        .env("TZ", "UTC0")
        .env("LC_ALL", "C")
        .output();
    if let Ok(out) = git
        && out.status.success()
    {
        build_time = String::from_utf8_lossy(&out.stdout).trim().to_string();
    }
    // The Makefile built both arches. Bazel writes each platform to bazel-bin in turn, so
    // each build is copied out before the next one replaces it.
    for arch in ["aarch64", "armhf"] {
        let platform = format!("--platforms=//platform/bazel/zig:{arch}_linux");
        let status = Command::new(&bazel)
            .args([
                "build",
                "-c",
                "opt",
                "--disk_cache=",
                &platform,
                &format!("--define=pulsar_version={version}"),
                &format!("--define=pulsar_build_time={build_time}"),
                "//projects/station/pulsar/satellite:station_binary",
            ])
            .current_dir(&repo)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => return fail(ui, &exit_status_text(&s)),
            Err(e) => return fail(ui, &e),
        }
        // The same --define flags: they are part of the configuration, so a different
        // set would point at another bazel-out directory.
        let info = Command::new(&bazel)
            .args([
                "info",
                "-c",
                "opt",
                "--disk_cache=",
                &platform,
                &format!("--define=pulsar_version={version}"),
                &format!("--define=pulsar_build_time={build_time}"),
                "bazel-bin",
            ])
            .current_dir(&repo)
            .output();
        let bin_dir = match info {
            Ok(o) if o.status.success() => {
                let text = String::from_utf8_lossy(&o.stdout);
                match text.lines().last().map(str::trim).filter(|l| !l.is_empty()) {
                    Some(p) => PathBuf::from(p),
                    None => return fail(ui, &"bazel info bazel-bin printed nothing"),
                }
            }
            Ok(o) => return fail(ui, &exit_status_text(&o.status)),
            Err(e) => return fail(ui, &e),
        };
        let out_dir = sat.join("build").join(arch);
        if let Err(e) = std::fs::create_dir_all(&out_dir) {
            return fail(ui, &e);
        }
        for name in ["pulsar", "pulsar.debug"] {
            let src = bin_dir.join("projects/station/pulsar/satellite").join(name);
            let Ok(data) = std::fs::read(&src) else {
                return fail(ui, &format!("{} missing", src.display()));
            };
            let dest = out_dir.join(name);
            if let Err(e) = std::fs::write(&dest, data) {
                return fail(ui, &e);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Err(e) =
                    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
                {
                    return fail(ui, &e);
                }
            }
        }
    }
    if ssh {
        for a in ["aarch64", "armhf"] {
            let p = root.join("ssh").join("out").join(a).join("dropbearmulti");
            if p.metadata().map_or(0, |m| m.len()) == 0 {
                let status = Command::new(root.join("ssh").join("build.sh"))
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .status();
                match status {
                    Ok(s) if s.success() => {}
                    Ok(s) => return fail(ui, &exit_status_text(&s)),
                    Err(e) => return fail(ui, &e),
                }
                break;
            }
        }
    }
    true
}

/// Copies the daemon, scripts, the radio's firmware images (zigbee) and dropbear (--ssh).
/// changed: the binary was replaced; sup_changed: supervise.sh differs from the installed
/// one or the Mini's asound.conf was replaced (the daemon reads it when it starts).
// The signature mirrors the Go function one-to-one; grouping the arguments would obscure
// which values are the station's.
#[allow(clippy::too_many_arguments)]
pub fn deploy_files(
    adb: &mut dyn Adb,
    root: &Path,
    ssh: bool,
    s: &str,
    bin: &Path,
    arch: &str,
    code: &str,
    zigbee: bool,
    ui: &mut Ui,
) -> (bool, bool, bool) {
    let mut changed = true;
    let mut sup_changed = true;
    let ssh_out = root.join("ssh").join("out").join(arch);
    let deploy = root.join("satellite").join("deploy");
    ui.line(&format!("== {s}: daemon ({arch})"));
    if adb
        .sh_to(ui.out, s, &format!("mkdir -p {DATA_DIR}"))
        .is_err()
    {
        return (changed, sup_changed, false);
    }
    let want = sha256_file(bin);
    let have = adb
        .sh(
            s,
            &format!("[ -f {DATA_DIR}/pulsar ] && sha256sum {DATA_DIR}/pulsar"),
        )
        .unwrap_or_default();
    if want == first_field(&have) {
        ui.line("binary identical to the installed one, not copied (.prev kept)");
        changed = false;
    } else {
        if adb.push(s, bin, &format!("{DATA_DIR}/pulsar.new")).is_err() {
            return (changed, sup_changed, false);
        }
        let got = adb
            .sh(s, &format!("sha256sum {DATA_DIR}/pulsar.new"))
            .unwrap_or_default();
        if want != first_field(&got) {
            let _ = adb.sh_to(ui.out, s, &format!("rm -f {DATA_DIR}/pulsar.new"));
            ui.error(&format!(
                "{s}: push corrupted (sha256 mismatch), nothing changed"
            ));
            return (changed, sup_changed, false);
        }
    }
    // Scripts: a running shell reads its script incrementally, so replace them atomically
    // (new inode). A running supervisor keeps executing the old inode: when supervise.sh
    // changed it is reloaded.
    let sup_have = adb
        .sh(
            s,
            &format!("[ -f {DATA_DIR}/supervise.sh ] && sha256sum {DATA_DIR}/supervise.sh"),
        )
        .unwrap_or_default();
    if first_field(&sup_have) == sha256_file(&deploy.join("supervise.sh")) {
        sup_changed = false;
    }
    for f in ["boot.sh", "supervise.sh", "ctl.sh"] {
        if adb
            .push(s, &deploy.join(f), &format!("{DATA_DIR}/{f}.new"))
            .is_err()
        {
            return (changed, sup_changed, false);
        }
    }
    if adb
        .sh_to(
            ui.out,
            s,
            &format!(
                "cd {DATA_DIR} && chmod 755 boot.sh.new supervise.sh.new ctl.sh.new &&\n     mv boot.sh.new boot.sh && mv supervise.sh.new supervise.sh && mv ctl.sh.new ctl.sh"
            ),
        )
        .is_err()
    {
        return (changed, sup_changed, false);
    }
    if code == "yandexmini" {
        // The Mini needs a dmix PCM over hw:0,0 so MUSIC, ANNOUNCE and Sendspin share the
        // speaker; the daemon selects /data/pulsar/asound.conf through ALSA_CONFIG_PATH.
        let want = sha256_file(&deploy.join("asound.conf"));
        let have = adb
            .sh(
                s,
                &format!("[ -f {DATA_DIR}/asound.conf ] && sha256sum {DATA_DIR}/asound.conf"),
            )
            .unwrap_or_default();
        if want.is_empty() || first_field(&have) != want {
            if adb
                .push(
                    s,
                    &deploy.join("asound.conf"),
                    &format!("{DATA_DIR}/asound.conf.new"),
                )
                .is_err()
            {
                return (changed, sup_changed, false);
            }
            let got = adb
                .sh(s, &format!("sha256sum {DATA_DIR}/asound.conf.new"))
                .unwrap_or_default();
            if want.is_empty() || want != first_field(&got) {
                let _ = adb.sh_to(ui.out, s, &format!("rm -f {DATA_DIR}/asound.conf.new"));
                ui.error(&format!(
                    "{s}: push corrupted (sha256 mismatch), asound.conf not replaced"
                ));
                return (changed, sup_changed, false);
            }
            if adb
                .sh_to(
                    ui.out,
                    s,
                    &format!(
                        "cd {DATA_DIR} && chmod 644 asound.conf.new && mv asound.conf.new asound.conf && sync"
                    ),
                )
                .is_err()
            {
                return (changed, sup_changed, false);
            }
            // The running daemon read the old config at start; reload the supervisor and daemon.
            sup_changed = true;
        }
    }
    if changed {
        if adb
            .sh_to(
                ui.out,
                s,
                &format!(
                    "cd {DATA_DIR} && chmod 755 pulsar.new &&\n       {{ [ ! -f pulsar ] || cp -p pulsar pulsar.prev; }} &&\n       mv pulsar.new pulsar && sync"
                ),
            )
            .is_err()
        {
            return (changed, sup_changed, false);
        }
    } else {
        let _ = adb.sh_to(ui.out, s, "sync");
    }

    // The Ricardo detector is a separate model, not part of the daemon binary.
    // A checkout without satellite/wakewords keeps whatever is installed.
    if !deploy_wake_models(adb, root, s, ui) {
        return (changed, sup_changed, false);
    }

    if zigbee
        && (!deploy_radio_image(adb, root, s, "zigbee-coordinator.ybl", ui)
            || !deploy_radio_image(adb, root, s, "zigbee-router.ybl", ui))
    {
        return (changed, sup_changed, false);
    }

    if ssh {
        ui.line(&format!("== {s}: dropbear"));
        let mut ok = adb.sh_to(ui.out, s, "mkdir -p /data/dropbear").is_ok();
        if ok {
            ok = adb
                .push(
                    s,
                    &ssh_out.join("dropbearmulti"),
                    "/data/dropbear/dropbearmulti.new",
                )
                .is_ok();
        }
        if ok {
            ok = adb
                .push(
                    s,
                    &ssh_out.join("sftp-server"),
                    "/data/dropbear/sftp-server.new",
                )
                .is_ok();
        }
        if ok {
            ok = adb
                .push(
                    s,
                    &root.join("ssh").join("start.sh"),
                    "/data/dropbear/start.sh",
                )
                .is_ok();
        }
        if ok {
            // Running binaries stay busy; mv replaces them atomically.
            ok = adb
                .sh_to(
                    ui.out,
                    s,
                    "cd /data/dropbear && chmod 755 dropbearmulti.new sftp-server.new start.sh &&\n       mv dropbearmulti.new dropbearmulti && mv sftp-server.new sftp-server && sync",
                )
                .is_ok();
        }
        if !ok {
            return (changed, sup_changed, false);
        }
    }

    if ssh {
        let _ = adb.sh_to(
            ui.out,
            s,
            "pidof dropbear >/dev/null || /data/dropbear/start.sh </dev/null >/dev/null 2>&1 &",
        );
    }
    (changed, sup_changed, true)
}

/// Copies every `*.tflite` from `satellite/wakewords` to `/data/pulsar/wakewords`. The model
/// is deliberately not part of the public source tree: a release or checkout that does not
/// contain it keeps the installed model, and a fresh install without one logs the missing
/// Ricardo detector instead of failing the whole daemon.
fn deploy_wake_models(adb: &mut dyn Adb, root: &Path, s: &str, ui: &mut Ui) -> bool {
    let dir = root.join("satellite").join("wakewords");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        ui.line("satellite/wakewords missing: no wake-word model copied");
        return true;
    };
    let mut found = false;
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_ok_and(|t| !t.is_dir())
                && go_ext(&e.file_name().to_string_lossy()) == ".tflite"
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names {
        found = true;
        let src = dir.join(&name);
        let dst = format!("{DATA_DIR}/wakewords/{name}");
        let want = sha256_file(&src);
        if want.is_empty() {
            ui.error(&format!("{s}: cannot read {}", src.display()));
            return false;
        }
        let have = adb
            .sh(s, &format!("[ -f {dst} ] && sha256sum {dst}"))
            .unwrap_or_default();
        if first_field(&have) == want {
            continue;
        }
        if adb
            .sh_to(ui.out, s, &format!("mkdir -p {DATA_DIR}/wakewords"))
            .is_err()
        {
            return false;
        }
        if adb.push(s, &src, &format!("{dst}.new")).is_err() {
            return false;
        }
        let got = adb
            .sh(s, &format!("sha256sum {dst}.new"))
            .unwrap_or_default();
        if first_field(&got) != want {
            let _ = adb.sh_to(ui.out, s, &format!("rm -f {dst}.new"));
            ui.error(&format!("{s}: wake model push corrupted (sha256 mismatch)"));
            return false;
        }
        if adb
            .sh_to(
                ui.out,
                s,
                &format!("chmod 644 {dst}.new && mv {dst}.new {dst} && sync"),
            )
            .is_err()
        {
            return false;
        }
        ui.line(&format!("wake model {name} copied"));
    }
    if !found {
        ui.line("satellite/wakewords has no *.tflite: no wake-word model copied");
    }
    true
}

/// Copies one of pulsar's radio firmware images, read by the daemon only when the Zigbee
/// role that needs it is chosen. A checkout without `radio/build.sh` output keeps the
/// installed one.
fn deploy_radio_image(adb: &mut dyn Adb, root: &Path, s: &str, name: &str, ui: &mut Ui) -> bool {
    let img = root.join("radio").join("out").join(name);
    if img.metadata().map_or(0, |m| m.len()) == 0 {
        ui.line(&format!(
            "radio/out/{name} missing (radio/build.sh), not copied"
        ));
        return true;
    }
    let want = sha256_file(&img);
    let have = adb
        .sh(
            s,
            &format!("[ -f {DATA_DIR}/{name} ] && sha256sum {DATA_DIR}/{name}"),
        )
        .unwrap_or_default();
    if first_field(&have) == want {
        return true;
    }
    if adb
        .push(s, &img, &format!("{DATA_DIR}/{name}.new"))
        .is_err()
    {
        return false;
    }
    let got = adb
        .sh(s, &format!("sha256sum {DATA_DIR}/{name}.new"))
        .unwrap_or_default();
    if first_field(&got) != want {
        let _ = adb.sh_to(ui.out, s, &format!("rm -f {DATA_DIR}/{name}.new"));
        ui.error(&format!(
            "{s}: push corrupted (sha256 mismatch), {name} not replaced"
        ));
        return false;
    }
    adb.sh_to(
        ui.out,
        s,
        &format!("cd {DATA_DIR} && mv {name}.new {name} && sync"),
    )
    .is_ok()
}

/// Makes `INIT_PATH` current, then removes the hooks older installers added to
/// rootkeeper.sh. note: autostart could not be written (said in the final summary); lost:
/// nothing starts pulsar at boot; !ok: station error.
pub fn boot_autostart(adb: &mut dyn Adb, s: &str, ts: &str, ui: &mut Ui) -> (String, bool, bool) {
    // Dropbear is installed by --ssh, in this run or an earlier one: never drop its autostart.
    let dropbear = adb.sh(s, "[ -x /data/dropbear/start.sh ]").is_ok();
    let want = init_script(dropbear);
    let want_sum = sha256_str(&want);
    let have = adb
        .sh(s, &format!("[ -f {INIT_PATH} ] && sha256sum {INIT_PATH}"))
        .unwrap_or_default();
    if first_field(&have) == want_sum {
        ui.line(&format!("{INIT_PATH} up to date"));
    } else {
        let (reason, ok) = write_init_script(adb, s, &want, &want_sum, ui);
        if !ok {
            return (String::new(), false, false);
        }
        if !reason.is_empty() {
            ui.line(&format!("{INIT_PATH} not written: {reason}"));
            let (note, lost) = autostart_fallback(adb, s, &reason, ui);
            return (note, lost, true);
        }
    }
    (String::new(), false, migrate_rootkeeper(adb, s, ts, ui))
}

/// Writes `want` to `INIT_PATH`. `reason != ""`: refused, nothing written (protected or
/// unreadable rootfs, remount failed); !ok: station error, reported on stderr.
fn write_init_script(
    adb: &mut dyn Adb,
    s: &str,
    want: &str,
    want_sum: &str,
    ui: &mut Ui,
) -> (String, bool) {
    let Ok(mounts) = adb.sh(s, "cat /proc/mounts") else {
        return ("cannot read /proc/mounts".to_string(), true);
    };
    let (rw, found) = root_mount_rw(&mounts);
    if !found {
        return ("/ not found in /proc/mounts".to_string(), true);
    }
    let Ok(cmdline) = adb.sh(s, "cat /proc/cmdline") else {
        return ("cannot read /proc/cmdline".to_string(), true);
    };
    // The pipe keeps ls from formatting for the pty (columns, colour).
    let mapper = adb
        .sh(s, "ls /dev/mapper 2>/dev/null | cat")
        .unwrap_or_default();
    let Ok(df) = adb.sh(s, "df /etc /system/vendor/quasar") else {
        return ("df /etc /system/vendor/quasar failed".to_string(), true);
    };
    let reason = rootfs_guard(&cmdline, &mapper, &df);
    if !reason.is_empty() {
        return (reason, true);
    }
    ui.line("rootfs: no dm-verity or device-mapper, /etc and /system/vendor/quasar on the root filesystem");

    let tmp = match tempfile::Builder::new().prefix("pulsar-install").tempdir() {
        Ok(t) => t,
        Err(e) => {
            ui.error(&e.to_string());
            return (String::new(), false);
        }
    };
    let file = tmp.path().join("S99zpulsar");
    if let Err(e) = std::fs::write(&file, want) {
        ui.error(&e.to_string());
        return (String::new(), false);
    }
    let src = format!("{DATA_DIR}/S99zpulsar.new");
    if adb.push(s, &file, &src).is_err() {
        return (String::new(), false);
    }
    // Everything from here on removes the staged file.
    let result = write_init_script_pushed(adb, s, &src, want_sum, rw, want, ui);
    let _ = adb.sh(s, &format!("rm -f {src}"));
    result
}

#[allow(clippy::too_many_arguments)]
fn write_init_script_pushed(
    adb: &mut dyn Adb,
    s: &str,
    src: &str,
    want_sum: &str,
    rw: bool,
    want: &str,
    ui: &mut Ui,
) -> (String, bool) {
    // Checked by the shell that runs it at boot (busybox sh).
    if adb.sh(s, &format!("sh -n {src}")).is_err() {
        ui.error(&format!(
            "{s}: {src} fails sh -n on the device, not installed"
        ));
        return (String::new(), false);
    }
    let out = adb.sh_raw(s, &write_init_cmd(src, want_sum, !rw));
    let (reason, ok) = match out {
        // The write command ends with `exit 1` after remount-rw-failed: the marker still
        // decides the refusal, exactly like Go (which inspected output before the error).
        Ok(o) if o.contains("remount-rw-failed") => {
            ("mount -o remount,rw / failed".to_string(), true)
        }
        Err(e) if e.out().contains("remount-rw-failed") => {
            ("mount -o remount,rw / failed".to_string(), true)
        }
        Ok(_) => {
            if first_field(
                &adb.sh(s, &format!("sha256sum {INIT_PATH}"))
                    .unwrap_or_default(),
            ) == want_sum
            {
                ui.line(&format!("wrote {INIT_PATH}:"));
                for l in split_lines(want) {
                    ui.line(&format!("  {l}"));
                }
                (String::new(), true)
            } else {
                ui.error(&format!(
                    "{s}: {INIT_PATH} on the device differs from the checked file"
                ));
                (String::new(), false)
            }
        }
        Err(e) => {
            ui.error(&format!("{s}: writing {INIT_PATH} failed:\n{}", e.out()));
            (String::new(), false)
        }
    };
    // Whatever happened in between, / must not stay read-write.
    if !rw {
        // Go ignores `found` here: an unreadable /proc/mounts also counts as done.
        let now = adb.sh(s, "cat /proc/mounts");
        if !now.as_ref().is_ok_and(|m| !root_mount_rw(m).0) {
            let _ = adb.sh(s, "mount -o remount,ro /");
            let now = adb.sh(s, "cat /proc/mounts");
            if !now.as_ref().is_ok_and(|m| !root_mount_rw(m).0) {
                ui.error(&format!(
                    "{s}: the root filesystem is still mounted read-write: run \"mount -o remount,ro /\" on the station or reboot it"
                ));
                return (reason, false);
            }
        }
    }
    (reason, ok)
}

/// After `INIT_PATH` could not be written, tells what still starts pulsar at boot.
fn autostart_fallback(adb: &mut dyn Adb, s: &str, reason: &str, ui: &mut Ui) -> (String, bool) {
    let hook = format!("grep -qxF '{HOOK_SAT}' {INIT_PATH}");
    if adb.sh(s, &hook).is_ok() {
        return (
            format!(
                "{INIT_PATH} not updated ({reason}); the installed one still starts pulsar at boot"
            ),
            false,
        );
    }
    let (rk, exists, ok) = read_rootkeeper(adb, s, ui);
    if ok
        && exists
        && crate::scripts::hook_active(&rk)
        && adb
            .sh(s, "grep -q rootkeeper.sh /system/vendor/quasar/onboot.sh")
            .is_ok()
    {
        return (
            format!(
                "{INIT_PATH} not written ({reason}); pulsar still starts at boot through the old hook in {RK_PATH}"
            ),
            false,
        );
    }
    (
        format!(
            "autostart NOT installed ({reason}): after a reboot pulsar does not start by itself and the Yandex stack runs; {DATA_DIR}/ctl.sh enable starts it again"
        ),
        true,
    )
}

/// `RK_PATH`'s content; `exists` is false when there is no such file.
fn read_rootkeeper(adb: &mut dyn Adb, s: &str, ui: &mut Ui) -> (String, bool, bool) {
    if adb.sh(s, &format!("[ -f {RK_PATH} ]")).is_err() {
        return (String::new(), false, true);
    }
    let tmp = match tempfile::Builder::new().prefix("pulsar-install").tempdir() {
        Ok(t) => t,
        Err(e) => {
            ui.error(&e.to_string());
            return (String::new(), true, false);
        }
    };
    let f = tmp.path().join("rk");
    if adb.pull(s, RK_PATH, &f).is_err() {
        return (String::new(), true, false);
    }
    match std::fs::read_to_string(&f) {
        Ok(b) => (b, true, true),
        Err(e) => {
            ui.error(&e.to_string());
            (String::new(), true, false)
        }
    }
}

/// Removes the hook lines older installers added to rootkeeper.sh (now in `INIT_PATH`),
/// keeping a backup. Never creates or deletes the file: it belongs to the root method.
fn migrate_rootkeeper(adb: &mut dyn Adb, s: &str, ts: &str, ui: &mut Ui) -> bool {
    let (rk, exists, ok) = read_rootkeeper(adb, s, ui);
    if !ok || !exists {
        return ok;
    }
    let (out, removed) = unhook_rootkeeper(&rk);
    if removed == 0 {
        return true;
    }
    // Nothing but our hook lines and the blank separators we added may go.
    let bad = check_rootkeeper(&out, &rk);
    if !bad.is_empty() {
        ui.error(&format!(
            "{s}: removing the pulsar lines would change more of {RK_PATH}, not written:\n{bad}"
        ));
        return false;
    }
    let tmp = match tempfile::Builder::new().prefix("pulsar-install").tempdir() {
        Ok(t) => t,
        Err(e) => {
            ui.error(&e.to_string());
            return false;
        }
    };
    let new_file = tmp.path().join("rk.new");
    if let Err(e) = std::fs::write(&new_file, &out) {
        ui.error(&e.to_string());
        return false;
    }
    if adb.push(s, &new_file, &format!("{RK_PATH}.new")).is_err() {
        return false;
    }
    if adb.sh(s, &format!("sh -n {RK_PATH}.new")).is_err() {
        let _ = adb.sh(s, &format!("rm -f {RK_PATH}.new"));
        ui.error(&format!(
            "{s}: {RK_PATH}.new fails sh -n on the device, not written"
        ));
        return false;
    }
    let bak = format!("{RK_PATH}.bak-{ts}");
    if adb
        .sh_to(
            ui.out,
            s,
            &format!(
                "cp -p {RK_PATH} {bak} && chmod 755 {RK_PATH}.new && mv {RK_PATH}.new {RK_PATH} && sync"
            ),
        )
        .is_err()
    {
        return false;
    }
    let want = sha256_file(&new_file);
    if first_field(
        &adb.sh(s, &format!("sha256sum {RK_PATH}"))
            .unwrap_or_default(),
    ) != want
    {
        ui.error(&format!(
            "{s}: {RK_PATH} on the device differs from the checked file"
        ));
        return false;
    }
    ui.line(&format!(
        "removed the old pulsar lines from {RK_PATH}, diff against {bak} on the device:"
    ));
    // busybox diff exits 1 when the files differ; its output is the point.
    let d = match adb.sh_raw(s, &format!("diff {bak} {RK_PATH}")) {
        Ok(d) => d,
        Err(e) => e.out().to_string(),
    };
    for l in split_lines(&d) {
        ui.line(&format!("  {l}"));
    }
    true
}

/// Runs after a (re)start, at ~10 s and again at ~30 s: the process is alive and, in the log
/// after the install marker, there is exactly one `=== start` and no crash report, exit or
/// supervisor message (a binary that crashes and is restarted in between would otherwise
/// look healthy). On failure, offers to put the previous binary back.
pub fn health_check(
    adb: &mut dyn Adb,
    s: &str,
    changed: bool,
    mark: &str,
    yes: bool,
    read_line: &mut dyn FnMut(&str) -> String,
    ui: &mut Ui,
) -> bool {
    let mut log = String::new();
    let mut bad = false;
    let mut unknown = false;
    'rounds: for round in 1..=2 {
        std::thread::sleep(if round == 1 {
            Duration::from_secs(10)
        } else {
            Duration::from_secs(20)
        });
        // The log may have been copy-truncated by supervise.sh meanwhile: the older half is in .1.
        let full = adb
            .sh(
                s,
                "cat /tmp/pulsar.log.1 /tmp/pulsar.log 2>/dev/null | tail -n 1500",
            )
            .unwrap_or_default();
        log = adb.sh(s, "tail -n 600 /tmp/pulsar.log").unwrap_or_default();
        if !log.contains(mark) {
            if full.contains(mark) {
                log = full;
            } else {
                log = String::new();
            }
        }
        if log.is_empty() {
            log = "(install marker not found in the log, even in .1)".to_string();
            unknown = true;
            break;
        }
        // The old daemon's exit (and the restart) land after the marker: judge only what
        // follows the first `=== start` after it, which is the new daemon's.
        let after = log
            .split_once(mark)
            .map_or(log.as_str(), |(_, after)| after);
        let Some(i) = after.find("=== start") else {
            bad = true;
            log = format!("(no start after the install marker){after}");
            break;
        };
        log = after[i..].to_string();
        let n = log
            .split('\n')
            .filter(|l| l.starts_with("=== start"))
            .count();
        // «=== alsa:» от supervise.sh — не сбой; см. docs/development.md, «Супервизор».
        if adb.sh_to(ui.out, s, "pidof pulsar >/dev/null").is_err()
            || n != 1
            || log.contains("=== CRASH")
            || log.contains("=== exit")
            || log.contains("=== supervise")
        {
            bad = true;
            break 'rounds;
        }
    }
    if unknown {
        ui.error(&format!(
            "{s}: health check UNKNOWN: {log}; not rolling back automatically"
        ));
        return false;
    }
    if !bad {
        ui.line(&format!("{s}: health check ok"));
        return true;
    }
    ui.error(&format!("{s}: health check FAILED, log:"));
    let mut lines: Vec<&str> = log.split('\n').collect();
    if lines.len() > 15 {
        lines = lines[lines.len() - 15..].to_vec();
    }
    ui.error(&lines.join("\n"));
    if changed
        && adb
            .sh_to(ui.out, s, &format!("[ -f {DATA_DIR}/pulsar.prev ]"))
            .is_ok()
    {
        let ans = if yes {
            "y".to_string()
        } else {
            read_line("roll back to the previous binary? [Y/n] ")
        };
        if !ans.starts_with('n') && !ans.starts_with('N') {
            let _ = adb.sh_to(
                ui.out,
                s,
                &format!(
                    "cd {DATA_DIR} && cp -p pulsar.prev pulsar.rb && mv pulsar pulsar.bad &&\n        mv pulsar.rb pulsar && sync && ./ctl.sh restart"
                ),
            );
            ui.error(&format!(
                "{s}: rolled back (bad binary kept as {DATA_DIR}/pulsar.bad)"
            ));
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
/// Deploys, (re)starts and checks one station; every step checks its own status. `note` is
/// set when autostart could not be (re)written; `lost`: the station has no autostart at all.
pub fn install_one(
    adb: &mut dyn Adb,
    root: &Path,
    opts: &crate::cli::Options,
    st: &Station,
    read_line: &mut dyn FnMut(&str) -> String,
    ui: &mut Ui,
) -> (bool, String, bool) {
    let s = st.serial.as_str();
    let bin = root
        .join("satellite")
        .join("build")
        .join(&st.arch)
        .join("pulsar");
    if bin.metadata().map_or(0, |m| m.len()) == 0 {
        ui.error(&format!("{s}: {} missing", bin.display()));
        return (false, String::new(), false);
    }
    let ts = timestamp();
    let (changed, sup_changed, ok) = deploy_files(
        adb, root, opts.ssh, s, &bin, &st.arch, &st.code, st.zigbee, ui,
    );
    if !ok {
        return (false, String::new(), false);
    }
    ui.line(&format!("== {s}: autostart"));
    let (note, lost, ok) = boot_autostart(adb, s, &ts, ui);
    if !ok {
        return (false, String::new(), false);
    }
    if opts.no_enable {
        return (true, note, lost);
    }
    ui.line(&format!("== {s}: start"));
    // Marker in the log: the health check looks only at what happens after it.
    let mark = format!(
        "=== install-marker {} {}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    );
    let _ = adb.sh_to(ui.out, s, &format!("echo '{mark}' >> /tmp/pulsar.log"));
    let action = if sup_changed { "reload" } else { "restart" };
    let _ = adb.sh_to(
        ui.out,
        s,
        &format!(
            "if [ -f {DATA_DIR}/enabled ] && {{ pidof supervise.sh || pidof pulsar; }} >/dev/null; then {DATA_DIR}/ctl.sh {action}; else {DATA_DIR}/ctl.sh enable; fi"
        ),
    );
    ui.blank();
    if !health_check(adb, s, changed, &mark, opts.yes, read_line, ui) {
        return (false, note, lost);
    }
    let _ = adb.sh_to(
        ui.out,
        s,
        &format!("{DATA_DIR}/ctl.sh status; {DATA_DIR}/ctl.sh log 5"),
    );
    ui.blank();
    (true, note, lost)
}

/// `YYYYMMDDHHMMSS`, local time like Go's `time.Now().Format`.
fn timestamp() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    format!(
        "{:04}{:02}{:02}{:02}{:02}{:02}",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// An installable station, in the order probe printed it.
#[derive(Debug, Clone)]
pub struct Station {
    pub serial: String,
    pub arch: String,
    pub code: String,
    pub zigbee: bool,
}

/// One status line for `s`; records it in `found` if it is installable. Returns `true` when a
/// device we connected to ourselves must be disconnected again (it is not installable).
pub fn probe(
    adb: &mut dyn Adb,
    s: &str,
    ours: bool,
    found: &mut Vec<Station>,
    ui: &mut Ui,
) -> bool {
    let skip = |ui: &mut Ui, msg: &str| {
        ui.line(&format!("  {s}: {msg}"));
        ours
    };
    let Ok(compat) = adb.sh(s, r"tr '\0' '\n' </proc/device-tree/compatible") else {
        return skip(ui, "adb not usable (unauthorized or offline)");
    };
    let (mut code, mut supported) = station_code(&compat);
    if code.is_empty() {
        // The first-generation Mini has no yandex,* compatible entry. axg_s420_v03 is an
        // Amlogic reference board, not a Yandex marker: require quasar.cfg's deviceType too
        // (the daemon and boot.sh use the same pair).
        let dtid = adb
            .sh(s, r"tr -d '\0' </proc/device-tree/amlogic-dt-id")
            .unwrap_or_default();
        let cfg = adb
            .sh(s, "cat /system/vendor/quasar/quasar.cfg 2>/dev/null")
            .unwrap_or_default();
        if dtid == "axg_s420_v03" && crate::probe::quasar_device_type(&cfg) == "yandexmini" {
            code = "yandexmini".to_string();
            supported = true;
        }
    }
    if code.is_empty() {
        return skip(ui, "not a Yandex Station");
    }
    let mut name = model_name(&code).to_string();
    if name.is_empty() {
        name = "unknown Yandex device".to_string();
    }
    name = format!("{name} ({code})");
    if !supported {
        return skip(ui, &format!("{name} — not supported yet, skipped"));
    }
    if adb.sh(s, "id -u").unwrap_or_default() != "0" {
        return skip(ui, &format!("{name} — adb is not root, skipped"));
    }
    // `; true`: a missing loader is not an error. Without it the last [ -x ] exits 1 on a
    // normal Midi (no armhf loader) and sh() reports a failure that is not an adb failure.
    let Ok(loaders) = adb.sh(
        s,
        r"[ -x /lib/ld-linux-aarch64.so.1 ] && echo aarch64; [ -x /lib/ld-linux-armhf.so.3 ] && echo armhf; true",
    ) else {
        return skip(ui, "adb not usable (unauthorized or offline)");
    };
    let loader_words: Vec<&str> = loaders.split_whitespace().collect();
    let arch = arch_of(&code, &loader_words);
    if arch.is_empty() {
        return skip(ui, &format!("{name} — unsupported userland ABI, skipped"));
    }
    let state = if adb.sh(s, "pidof pulsar >/dev/null").is_ok() {
        "satellite running"
    } else if adb.sh(s, &format!("[ -x {DATA_DIR}/pulsar ]")).is_ok() {
        "satellite installed, not running"
    } else {
        "stock Yandex"
    };
    found.push(Station {
        serial: s.to_string(),
        arch: arch.to_string(),
        code: code.clone(),
        zigbee: has_zigbee(&code),
    });
    let note = if arch == crate::probe::abis(&code)[0] {
        String::new()
    } else {
        format!(", {arch} build")
    };
    ui.line(&format!("  [{}] {s}: {name} — {state}{note}", found.len()));
    false
}

#[cfg(test)]
mod tests;
