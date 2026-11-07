//! Golden tests for the deploy sequence: a fake `adb` records every call exactly like the
//! Go generator did and replays the same answers. It can also return non-zero statuses with
//! output (busybox `diff`, read-only rootfs, failed `ctl.sh`) to keep those paths covered.

use super::*;
use crate::adb::ShError;
use crate::cli::Options;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io;
use std::path::{Path, PathBuf};

fn sha(p: &Path) -> String {
    let d = std::fs::read(p).unwrap();
    hex::encode(Sha256::digest(&d))
}

fn init_digest(with_ssh: bool) -> String {
    hex::encode(Sha256::digest(init_script(with_ssh).as_bytes()))
}

fn exit(code: &str, out: &str) -> Result<String, ShError> {
    Err(ShError::Exit {
        code: code.to_string(),
        out: out.to_string(),
    })
}

struct Fake {
    root: PathBuf,
    ssh: bool,
    rk_in: String,
    rk_out: String,
    calls: Vec<Vec<String>>,
    serial: String,
    mounts_rw: bool,
    cmdline: String,
    rootkeeper_exists: bool,
    init_installed: bool,
    marker: Option<String>,
}

impl Fake {
    fn new(root: &Path, ssh: bool, rk_in: &str) -> Self {
        let (rk_out, _) = unhook_rootkeeper(rk_in);
        Self {
            root: root.to_path_buf(),
            ssh,
            rk_in: rk_in.to_string(),
            rk_out,
            calls: Vec::new(),
            serial: "adb-1".to_string(),
            mounts_rw: true,
            cmdline: "console=ttyS0 root=PARTLABEL=system rootfstype=ext4".to_string(),
            rootkeeper_exists: true,
            init_installed: false,
            marker: None,
        }
    }

    fn record(&mut self, args: &[String]) {
        let mut v = vec!["-s".to_string(), self.serial.clone()];
        v.extend_from_slice(args);
        self.calls.push(v);
    }

    fn digest(&self, path: &str) -> String {
        let p = match path {
            "/data/pulsar/pulsar.new" | "/data/pulsar/pulsar" => {
                self.root.join("satellite/build/aarch64/pulsar")
            }
            "/data/pulsar/supervise.sh" => self.root.join("satellite/deploy/supervise.sh"),
            "/data/pulsar/asound.conf.new" | "/data/pulsar/asound.conf" => {
                self.root.join("satellite/deploy/asound.conf")
            }
            "/data/pulsar/wakewords/ru_ricardo_mww.tflite.new" => {
                self.root.join("satellite/wakewords/ru_ricardo_mww.tflite")
            }
            "/data/pulsar/zigbee-coordinator.ybl.new" => {
                self.root.join("radio/out/zigbee-coordinator.ybl")
            }
            "/data/pulsar/zigbee-router.ybl.new" => self.root.join("radio/out/zigbee-router.ybl"),
            _ => return String::new(),
        };
        format!("{}  {path}", sha(&p))
    }

    fn log(&self) -> String {
        self.marker
            .as_ref()
            .map_or_else(String::new, |m| format!("{m}\n=== start\nrunning\n"))
    }
}

fn respond(f: &mut Fake, cmd: &str) -> Result<String, ShError> {
    f.record(&["shell".to_string(), format!("{cmd}\necho __rc=$?")]);
    let plain = cmd.strip_suffix("\necho __rc=$?").unwrap_or(cmd);
    if plain == "[ -x /data/dropbear/start.sh ]" {
        return if f.ssh {
            Ok(String::new())
        } else {
            exit("1", "")
        };
    }
    if plain.starts_with("grep -qxF") || plain.starts_with("grep -q rootkeeper.sh") {
        return if f.init_installed {
            Ok(String::new())
        } else {
            exit("1", "")
        };
    }
    if plain == "[ -f /data/quasar/rootkeeper.sh ]" {
        return if f.rootkeeper_exists {
            Ok(String::new())
        } else {
            exit("1", "")
        };
    }
    if plain.starts_with("echo '") && plain.contains("=== install-marker") {
        if let Some(start) = plain.find('\'') {
            let rest = &plain[start + 1..];
            if let Some(end) = rest.find('\'') {
                f.marker = Some(rest[..end].to_string());
            }
        }
        return Ok(String::new());
    }
    if plain.starts_with("( trap '' HUP INT") {
        return if f.mounts_rw {
            Ok("wrote".to_string())
        } else {
            // busybox remount fails, prints the marker and exits 1.
            exit("1", "remount-rw-failed\n")
        };
    }
    if plain.starts_with("diff ") {
        // busybox diff exits 1 when files differ; the output is still the diff.
        return exit("1", &format!("--- bak\n+++ new\n-{HOOK_SAT}\n\n"));
    }
    let out = match plain {
        "cat /proc/mounts" => format!(
            "/dev/root / ext4 {},relatime 0 0",
            if f.mounts_rw { "rw" } else { "ro" }
        ),
        "cat /proc/cmdline" => f.cmdline.clone(),
        "ls /dev/mapper 2>/dev/null | cat" => "control".to_string(),
        "df /etc /system/vendor/quasar" => {
            "Filesystem 1K-blocks Used Available Use% Mounted on\n/dev/root 998060 389024 540224 42% /\n/dev/root 998060 389024 540224 42% /".to_string()
        }
        p if p == format!("sha256sum {INIT_PATH}") => init_digest(f.ssh),
        "sha256sum /data/quasar/rootkeeper.sh" => {
            format!("{}  x", hex::encode(Sha256::digest(f.rk_out.as_bytes())))
        }
        p if p.starts_with("sha256sum ") => {
            let path = &p["sha256sum ".len()..];
            f.digest(path)
        }
        p if p.starts_with("cp -p /data/quasar/rootkeeper.sh") => String::new(),
        "pidof pulsar >/dev/null" => String::new(),
        p if p.starts_with("cat /tmp/pulsar.log") || p.starts_with("tail -n 600") => f.log(),
        p if p.starts_with("if [ -f /data/pulsar/enabled ]") => String::new(),
        p if p.starts_with("/data/pulsar/ctl.sh") => "status ok\n".to_string(),
        _ => String::new(),
    };
    Ok(out)
}

impl Adb for Fake {
    fn push(&mut self, _serial: &str, local: &Path, remote: &str) -> io::Result<()> {
        self.record(&[
            "push".to_string(),
            local.display().to_string(),
            remote.to_string(),
        ]);
        Ok(())
    }

    fn pull(&mut self, _serial: &str, remote: &str, local: &Path) -> io::Result<()> {
        self.record(&[
            "pull".to_string(),
            remote.to_string(),
            local.display().to_string(),
        ]);
        std::fs::write(local, &self.rk_in)
    }

    fn sh_raw(&mut self, _serial: &str, cmd: &str) -> Result<String, ShError> {
        respond(self, cmd)
    }
}

fn make_root(dir: &Path) -> PathBuf {
    let root = dir.join("root");
    let sat = root.join("satellite");
    for d in [
        sat.join("deploy"),
        sat.join("build/aarch64"),
        sat.join("wakewords"),
        root.join("radio/out"),
        root.join("ssh/out/aarch64"),
    ] {
        std::fs::create_dir_all(d).unwrap();
    }
    let w = |p: PathBuf, s: &str| std::fs::write(p, s).unwrap();
    w(sat.join("build/aarch64/pulsar"), "daemon binary\n");
    w(sat.join("deploy/boot.sh"), "#!/bin/sh\nboot\n");
    w(sat.join("deploy/supervise.sh"), "#!/bin/sh\nsupervise\n");
    w(sat.join("deploy/ctl.sh"), "#!/bin/sh\nctl\n");
    w(sat.join("deploy/asound.conf"), "pcm.dmix {}\n");
    w(sat.join("wakewords/ru_ricardo_mww.tflite"), "wake model\n");
    w(
        root.join("radio/out/zigbee-coordinator.ybl"),
        "coordinator\n",
    );
    w(root.join("radio/out/zigbee-router.ybl"), "router\n");
    w(root.join("ssh/out/aarch64/dropbearmulti"), "dropbear\n");
    w(root.join("ssh/out/aarch64/sftp-server"), "sftp\n");
    w(root.join("ssh/start.sh"), "#!/bin/sh\nstart dropbear\n");
    root
}

fn normalize(root: &Path, s: &str) -> String {
    let mut s = s.replace(&root.display().to_string(), "<ROOT>");
    if let Some(i) = s.find("/pulsar-install") {
        let rest = &s[i + 1..];
        let mut parts = rest.splitn(2, '/');
        let _first = parts.next();
        let tail = parts.next().unwrap_or("");
        s = if tail.is_empty() {
            "<TMP>/pulsar-install".to_string()
        } else {
            format!("<TMP>/pulsar-install/{tail}")
        };
    }
    s
}

type DeployRun = (
    Vec<Vec<String>>,
    String,
    String,
    (bool, bool, bool),
    String,
    bool,
    bool,
);

fn run_deploy(ssh: bool) -> DeployRun {
    let dir = tempfile::tempdir().unwrap();
    let root = make_root(dir.path());
    let rk_in = format!("#!/bin/sh\necho owner\n{HOOK_SAT}\n\nwhile true; do\n  sleep 60\ndone\n");
    let mut fake = Fake::new(&root, ssh, &rk_in);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let st = Station {
        serial: "adb-1".to_string(),
        arch: "aarch64".to_string(),
        code: "cucumber".to_string(),
        zigbee: true,
    };
    let mut ui = Ui {
        out: &mut out,
        err: &mut err,
    };
    let (changed, sup, ok) = deploy_files(
        &mut fake,
        &root,
        ssh,
        &st.serial,
        &root.join("satellite/build/aarch64/pulsar"),
        &st.arch,
        &st.code,
        st.zigbee,
        &mut ui,
    );
    let (note, lost, ok2) = boot_autostart(&mut fake, &st.serial, "20240101120000", &mut ui);
    let calls = fake
        .calls
        .iter()
        .map(|c| c.iter().map(|a| normalize(&root, a)).collect())
        .collect();
    (
        calls,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
        (changed, sup, ok && ok2),
        note,
        lost,
        ok2,
    )
}

fn golden(name: &str) -> Value {
    let text = match name {
        "deploy_nossh.json" => include_str!("../../testdata/deploy_nossh.json"),
        "deploy_ssh.json" => include_str!("../../testdata/deploy_ssh.json"),
        "health_ok.json" => include_str!("../../testdata/health_ok.json"),
        "health_no_marker.json" => include_str!("../../testdata/health_no_marker.json"),
        _ => panic!("unknown fixture {name}"),
    };
    serde_json::from_str(text).unwrap()
}

#[test]
fn deploy_without_ssh_matches_go_golden() {
    let (calls, out, err, flags, note, lost, ok2) = run_deploy(false);
    let g = golden("deploy_nossh.json");
    assert_eq!(
        serde_json::to_value(&calls).unwrap(),
        g["calls"],
        "call sequence differs"
    );
    assert_eq!(out, g["stdout"].as_str().unwrap(), "stdout differs");
    assert_eq!(err, g["stderr"].as_str().unwrap(), "stderr differs");
    assert_eq!(
        flags,
        (
            g["changed"].as_bool().unwrap(),
            g["sup_changed"].as_bool().unwrap(),
            g["ok"].as_bool().unwrap(),
        )
    );
    assert_eq!(note, g["note"].as_str().unwrap());
    assert_eq!(lost, g["lost"].as_bool().unwrap());
    assert!(ok2);
}

#[test]
fn deploy_with_ssh_matches_go_golden() {
    let (calls, out, err, flags, note, lost, _ok2) = run_deploy(true);
    let g = golden("deploy_ssh.json");
    assert_eq!(serde_json::to_value(&calls).unwrap(), g["calls"]);
    assert_eq!(out, g["stdout"].as_str().unwrap());
    assert_eq!(err, g["stderr"].as_str().unwrap());
    assert_eq!(
        flags,
        (
            g["changed"].as_bool().unwrap(),
            g["sup_changed"].as_bool().unwrap(),
            g["ok"].as_bool().unwrap(),
        )
    );
    assert_eq!(note, g["note"].as_str().unwrap());
    assert_eq!(lost, g["lost"].as_bool().unwrap());
}

fn run_boot(fake: &mut Fake) -> (String, bool, bool, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut ui = Ui {
        out: &mut out,
        err: &mut err,
    };
    let (note, lost, ok) = boot_autostart(fake, "adb-1", "20240101120000", &mut ui);
    (note, lost, ok, String::from_utf8(out).unwrap())
}

#[test]
fn read_only_rootfs_reports_remount_failure_and_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    let root = make_root(dir.path());
    let mut fake = Fake::new(&root, false, "");
    fake.mounts_rw = false;
    fake.rootkeeper_exists = false;
    let (note, lost, ok, out) = run_boot(&mut fake);
    assert!(ok);
    assert!(lost);
    assert!(note.contains("mount -o remount,rw / failed"), "{note}");
    assert!(
        out.contains("not written: mount -o remount,rw / failed"),
        "{out}"
    );
}

#[test]
fn protected_rootfs_skips_autostart() {
    let dir = tempfile::tempdir().unwrap();
    let root = make_root(dir.path());
    let mut fake = Fake::new(&root, false, "");
    fake.cmdline = "root=/dev/root androidboot.veritymode=enforcing".to_string();
    fake.rootkeeper_exists = false;
    let (note, lost, ok, out) = run_boot(&mut fake);
    assert!(ok);
    assert!(lost);
    assert!(
        note.contains("kernel command line has androidboot.veritymode=enforcing"),
        "{note}"
    );
    assert!(out.contains("not written:"), "{out}");
}

#[test]
fn install_one_starts_and_checks_health() {
    let dir = tempfile::tempdir().unwrap();
    let root = make_root(dir.path());
    let rk_in = format!("#!/bin/sh\necho owner\n{HOOK_SAT}\n\nwhile true; do\n  sleep 60\ndone\n");
    let mut fake = Fake::new(&root, false, &rk_in);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut ui = Ui {
        out: &mut out,
        err: &mut err,
    };
    let st = Station {
        serial: "adb-1".into(),
        arch: "aarch64".into(),
        code: "cucumber".into(),
        zigbee: true,
    };
    let opts = Options {
        yes: true,
        ..Options::default()
    };
    let (ok, note, lost) = install_one(&mut fake, &root, &opts, &st, &mut |_| "y".into(), &mut ui);
    assert!(ok, "install failed: {}", String::from_utf8_lossy(&err));
    assert!(!lost);
    assert_eq!(note, "");
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("== adb-1: start"), "{out}");
    assert!(out.contains("adb-1: health check ok"), "{out}");
    let calls = fake.calls.iter().map(|c| c.join(" ")).collect::<Vec<_>>();
    assert!(
        calls.iter().any(|c| c.contains("install-marker")),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|c| c.contains("ctl.sh status; /data/pulsar/ctl.sh log 5")),
        "{calls:?}"
    );
}

struct HealthFake {
    log: String,
    calls: Vec<String>,
}

impl Adb for HealthFake {
    fn push(&mut self, _: &str, _: &Path, _: &str) -> io::Result<()> {
        Ok(())
    }
    fn pull(&mut self, _: &str, _: &str, _: &Path) -> io::Result<()> {
        Ok(())
    }
    fn sh_raw(&mut self, _: &str, cmd: &str) -> Result<String, ShError> {
        self.calls.push(cmd.to_string());
        Ok(
            if cmd.contains("tail")
                || cmd.starts_with("cat /tmp")
                || cmd == "pidof pulsar >/dev/null"
            {
                // Go's fake echoed the log and then a newline before the marker.
                format!("{}\n", self.log)
            } else {
                String::new()
            },
        )
    }
}

#[test]
fn health_check_matches_go_golden() {
    for name in ["health_ok", "health_no_marker"] {
        let g = golden(&format!("{name}.json"));
        let mut adb = HealthFake {
            log: g["log"].as_str().unwrap().to_string(),
            calls: Vec::new(),
        };
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut ui = Ui {
            out: &mut out,
            err: &mut err,
        };
        let got = health_check(
            &mut adb,
            "adb-1",
            true,
            "mark-1",
            true,
            &mut |_| "y".into(),
            &mut ui,
        );
        assert_eq!(got, g["want"].as_bool().unwrap(), "{name}");
        assert_eq!(
            String::from_utf8(out).unwrap(),
            g["stdout"].as_str().unwrap(),
            "{name} stdout"
        );
        assert_eq!(
            String::from_utf8(err).unwrap(),
            g["stderr"].as_str().unwrap(),
            "{name} stderr"
        );
    }
}

#[test]
fn probe_records_supported_station() {
    struct P;
    impl Adb for P {
        fn push(&mut self, _: &str, _: &Path, _: &str) -> io::Result<()> {
            Ok(())
        }
        fn pull(&mut self, _: &str, _: &str, _: &Path) -> io::Result<()> {
            Ok(())
        }
        fn sh_raw(&mut self, _: &str, cmd: &str) -> Result<String, ShError> {
            let c = cmd.strip_suffix("\necho __rc=$?").unwrap_or(cmd);
            Ok(if c.contains("compatible") {
                "yandex,cucumber-dvt2\nyandex,cucumber\namlogic, a5".to_string()
            } else if c == "id -u" {
                "0".to_string()
            } else if c.contains("ld-linux") {
                "aarch64".to_string()
            } else {
                String::new()
            })
        }
    }
    let mut adb = P;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut found = Vec::new();
    let mut ui = Ui {
        out: &mut out,
        err: &mut err,
    };
    probe(&mut adb, "192.0.2.10:5555", false, &mut found, &mut ui);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].arch, "aarch64");
    assert_eq!(found[0].code, "cucumber");
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "  [1] 192.0.2.10:5555: Yandex Station Midi (cucumber) — satellite running\n"
    );
}

#[test]
fn install_one_reports_missing_binary() {
    let dir = tempfile::tempdir().unwrap();
    let mut adb = Fake::new(dir.path(), false, "");
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut ui = Ui {
        out: &mut out,
        err: &mut err,
    };
    let st = Station {
        serial: "adb-1".into(),
        arch: "aarch64".into(),
        code: "plum".into(),
        zigbee: false,
    };
    let opts = Options::default();
    let (_ok, _note, _lost) = install_one(
        &mut adb,
        dir.path(),
        &opts,
        &st,
        &mut |_| String::new(),
        &mut ui,
    );
    let err = String::from_utf8(err).unwrap();
    assert!(err.contains("missing"), "{err}");
}

#[test]
fn probe_names_and_skips_unsupported_device() {
    struct P;
    impl Adb for P {
        fn push(&mut self, _: &str, _: &Path, _: &str) -> io::Result<()> {
            Ok(())
        }
        fn pull(&mut self, _: &str, _: &str, _: &Path) -> io::Result<()> {
            Ok(())
        }
        fn sh_raw(&mut self, _: &str, cmd: &str) -> Result<String, ShError> {
            let c = cmd.strip_suffix("\necho __rc=$?").unwrap_or(cmd);
            Ok(if c.contains("compatible") {
                "yandex,station\namlogic, axg".to_string()
            } else {
                String::new()
            })
        }
    }
    let mut adb = P;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut found = Vec::new();
    let mut ui = Ui {
        out: &mut out,
        err: &mut err,
    };
    let disconnect = probe(&mut adb, "192.0.2.11:5555", true, &mut found, &mut ui);
    assert!(found.is_empty());
    assert!(disconnect);
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "  192.0.2.11:5555: unknown Yandex device (station) — not supported yet, skipped\n"
    );
}
