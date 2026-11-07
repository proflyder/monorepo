//! Pure shell text the installer produces or inspects: the init script, the
//! rootfs guard, `/proc/mounts` parsing and the old `rootkeeper.sh` hooks.
//!
//! These functions do no I/O: every input is a string read by the caller and
//! every output is compared against the Go installer's golden fixtures in
//! `testdata/`.

/// Lines that start pulsar / dropbear at boot: the body of S99zpulsar. Older installers
/// put the same lines into /data/quasar/rootkeeper.sh (removed by `unhook_rootkeeper`).
pub const HOOK_SAT: &str =
    "[ -x /data/pulsar/boot.sh ] && /data/pulsar/boot.sh </dev/null >/dev/null 2>&1 &";
pub const HOOK_SSH: &str =
    "[ -x /data/dropbear/start.sh ] && /data/dropbear/start.sh </dev/null >/dev/null 2>&1 &";

/// rcS runs /etc/init.d/S??* in order with "start" and waits for each. The name sorts after
/// the firmware's S97quasar / S99quasar (S99pulsar would sort before S99quasar). The only
/// file pulsar writes outside /data.
pub const INIT_PATH: &str = "/etc/init.d/S99zpulsar";

/// Content of `INIT_PATH`. rcS has no controlling tty (inittab ::sysinit:), so the
/// background hooks get no SIGHUP when it exits.
pub fn init_script(with_ssh: bool) -> String {
    let mut s = String::new();
    s.push_str("#!/bin/sh\n");
    s.push_str("# pulsar autostart, written by pulsar-install; remove to disable autostart.\n");
    s.push_str("[ \"$1\" = start ] || exit 0\n");
    if with_ssh {
        s.push_str(HOOK_SSH);
        s.push('\n');
    }
    s.push_str(HOOK_SAT);
    s.push('\n');
    s
}

fn has_verity_token(tok: &str) -> bool {
    let lower = tok.to_ascii_lowercase();
    lower.contains("verity") || lower.contains("dm=")
}

/// `""` if the root filesystem may be written, else the reason it must not be:
/// dm-verity or any device-mapper target (writing under it corrupts the verified image), or
/// /etc not on the filesystem mounted at / together with the firmware. `cmdline` =
/// /proc/cmdline, `mapper` = `ls /dev/mapper`, `df` = `df /etc /system/vendor/quasar`.
pub fn rootfs_guard(cmdline: &str, mapper: &str, df: &str) -> String {
    for tok in cmdline.split_whitespace() {
        if has_verity_token(tok) {
            return format!("kernel command line has {tok}");
        }
    }
    let names: Vec<&str> = mapper
        .split_whitespace()
        .filter(|n| *n != "control")
        .collect();
    if !names.is_empty() {
        return format!("device-mapper devices present: {}", names.join(" "));
    }
    let rest = df
        .strip_prefix("Filesystem")
        .map_or(df, |r| match r.split_once('\n') {
            Some((_, rest)) => rest,
            None => "",
        });
    // Fields also joins the lines df wraps after a long device name.
    let f: Vec<&str> = rest.split_whitespace().collect();
    if f.len() != 12 {
        return "cannot parse df output".to_string();
    }
    if f[0] != f[6] || f[5] != f[11] {
        return format!(
            "/etc ({} on {}) and /system/vendor/quasar ({} on {}) are on different filesystems",
            f[0], f[5], f[6], f[11]
        );
    }
    if f[5] != "/" {
        return format!("/etc is on {}, not on the root filesystem", f[5]);
    }
    String::new()
}

/// Whether `/` is mounted read-write according to /proc/mounts. The last entry for / wins:
/// an initramfs "rootfs / rootfs rw" line can come before the real one.
pub fn root_mount_rw(mounts: &str) -> (bool, bool) {
    let mut rw = false;
    let mut found = false;
    for line in mounts.split('\n') {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 4 || f[1] != "/" {
            continue;
        }
        let opt = f[3].split(',').next().unwrap_or("");
        rw = opt == "rw";
        found = true;
    }
    (rw, found)
}

/// One device command, so the read-write window never spans two adb calls. The ignored HUP
/// lets it finish if adb drops; the dot name keeps rcS's S??* glob from running a partial
/// file. The subshell keeps exit from skipping `sh_raw`'s status echo.
pub fn write_init_cmd(src: &str, sum: &str, remount: bool) -> String {
    let mut l = vec!["( trap '' HUP INT".to_string()];
    if remount {
        l.push("mount -o remount,rw / || { echo remount-rw-failed; exit 1; }".to_string());
    }
    l.push("T=/etc/init.d/.S99zpulsar.tmp".to_string());
    l.push(format!(
        "cp {src} $T && chmod 755 $T && set -- $(sha256sum $T) && [ \"$1\" = {sum} ] && mv $T {INIT_PATH} && sync"
    ));
    l.push("rc=$?".to_string());
    l.push("rm -f $T".to_string());
    if remount {
        l.push("mount -o remount,ro / || rc=1".to_string());
    }
    l.push("exit $rc )".to_string());
    l.join("\n")
}

pub fn split_lines(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    s.strip_suffix('\n')
        .unwrap_or(s)
        .split('\n')
        .map(str::to_string)
        .collect()
}

fn join_lines(lines: &[String]) -> String {
    let mut b = String::new();
    for l in lines {
        b.push_str(l);
        b.push('\n');
    }
    b
}

/// The script is shared with other tooling and ends in an endless `while` loop: code after
/// it never runs.
fn is_loop(l: &str) -> bool {
    l.starts_with("while ") || l.starts_with("while\t")
}

fn is_hook(l: &str) -> bool {
    l == HOOK_SAT || l == HOOK_SSH
}

/// `rk` without the hook lines older installers added, and the number removed. Hook lines go
/// wherever they are, including dead ones after the loop. Blank lines go only in the two
/// shapes older installers wrote: hooks then a blank line right before the first loop, or a
/// blank line then hooks at the end of the file. Every other byte stays.
pub fn unhook_rootkeeper(rk: &str) -> (String, usize) {
    let lines = split_lines(rk);
    let mut drop = vec![false; lines.len()];
    let mut removed = 0;
    for (i, l) in lines.iter().enumerate() {
        if is_hook(l) {
            drop[i] = true;
            removed += 1;
        }
    }
    if removed == 0 {
        return (rk.to_string(), 0);
    }
    if let Some(lo) = lines.iter().position(|l| is_loop(l)) {
        let mut j = lo as i64 - 1;
        while j >= 0 && (is_hook(&lines[j as usize]) || lines[j as usize].is_empty()) {
            let u = j as usize;
            if lines[u].is_empty() && u > 0 && is_hook(&lines[u - 1]) {
                drop[u] = true;
            }
            j -= 1;
        }
    }
    let mut j = lines.len() as i64 - 1;
    while j >= 0 && (is_hook(&lines[j as usize]) || lines[j as usize].is_empty()) {
        let u = j as usize;
        if lines[u].is_empty() && u + 1 < lines.len() && is_hook(&lines[u + 1]) {
            drop[u] = true;
        }
        j -= 1;
    }
    let kept: Vec<String> = lines
        .into_iter()
        .zip(drop)
        .filter_map(|(l, d)| (!d).then_some(l))
        .collect();
    let mut out = join_lines(&kept);
    if !rk.ends_with('\n') {
        out.pop();
    }
    (out, removed)
}

/// Whether `rk` starts pulsar: a hook line before the endless loop (code after it never
/// runs).
pub fn hook_active(rk: &str) -> bool {
    for l in split_lines(rk) {
        if is_loop(&l) {
            return false;
        }
        if l == HOOK_SAT {
            return true;
        }
    }
    false
}

/// `""` if `with` is `without` plus only hook lines and blank lines, else a description of
/// the first difference ("- <line of without>\n+ <line of with>").
pub fn check_rootkeeper(without: &str, with: &str) -> String {
    let a = split_lines(without);
    let b = split_lines(with);
    let mut i = 0;
    for l in &b {
        if i < a.len() && *l == a[i] {
            i += 1;
        } else if is_hook(l) || l.is_empty() {
            continue;
        } else if i < a.len() {
            return format!("- {}\n+ {}", a[i], l);
        } else {
            return format!("+ {l}");
        }
    }
    if i < a.len() {
        return format!("- {}", a[i]);
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture(name: &str) -> Value {
        let text = match name {
            "write_init_cmd.json" => include_str!("../testdata/write_init_cmd.json"),
            "rootfs_guard.json" => include_str!("../testdata/rootfs_guard.json"),
            "root_mount.json" => include_str!("../testdata/root_mount.json"),
            "rootkeeper.json" => include_str!("../testdata/rootkeeper.json"),
            "hook_active.json" => include_str!("../testdata/hook_active.json"),
            _ => panic!("unknown fixture {name}"),
        };
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn init_script_matches_go_golden() {
        assert_eq!(
            init_script(false),
            include_str!("../testdata/init_script_nossh.txt")
        );
        assert_eq!(
            init_script(true),
            include_str!("../testdata/init_script_ssh.txt")
        );
    }

    #[test]
    fn write_init_cmd_matches_go_golden() {
        let cases = fixture("write_init_cmd.json");
        for c in cases.as_array().unwrap() {
            let remount = c["remount"].as_str().unwrap() == "true";
            assert_eq!(
                write_init_cmd(
                    c["src"].as_str().unwrap(),
                    c["sum"].as_str().unwrap(),
                    remount
                ),
                c["cmd"].as_str().unwrap()
            );
        }
    }

    #[test]
    fn rootfs_guard_matches_go_golden() {
        for c in fixture("rootfs_guard.json").as_array().unwrap() {
            let got = rootfs_guard(
                c["cmdline"].as_str().unwrap(),
                c["mapper"].as_str().unwrap(),
                c["df"].as_str().unwrap(),
            );
            assert_eq!(got, c["result"].as_str().unwrap(), "case {}", c["name"]);
        }
    }

    #[test]
    fn root_mount_matches_go_golden() {
        for c in fixture("root_mount.json").as_array().unwrap() {
            assert_eq!(
                root_mount_rw(c["mounts"].as_str().unwrap()),
                (c["rw"].as_bool().unwrap(), c["found"].as_bool().unwrap())
            );
        }
    }

    #[test]
    fn unhook_matches_go_golden() {
        for c in fixture("rootkeeper.json").as_array().unwrap() {
            let input = c["in"].as_str().unwrap();
            let (out, removed) = unhook_rootkeeper(input);
            assert_eq!(out, c["out"].as_str().unwrap());
            assert_eq!(removed as u64, c["removed"].as_u64().unwrap());
            if removed > 0 {
                assert_eq!(check_rootkeeper(&out, input), "");
            }
        }
    }

    #[test]
    fn hook_active_matches_go_golden() {
        for c in fixture("hook_active.json").as_array().unwrap() {
            assert_eq!(
                hook_active(c["rk"].as_str().unwrap()),
                c["want"].as_bool().unwrap()
            );
        }
    }

    #[test]
    fn check_rootkeeper_rejects_changes() {
        let input =
            format!("#!/bin/sh\necho start\n{HOOK_SAT}\n\nwhile true; do\n  sleep 60\ndone\n");
        let (out, _) = unhook_rootkeeper(&input);
        assert_ne!(
            check_rootkeeper(&out.replace("sleep 60", "sleep 61"), &input),
            ""
        );
        assert_ne!(
            check_rootkeeper(&out.replace("echo start\n", ""), &input),
            ""
        );
    }
}
