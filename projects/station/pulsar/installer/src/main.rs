//! Entry point: the install flow. Every message and exit code matches the Go installer.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pulsar_installer::adb::ProcessAdb;
use pulsar_installer::cli::{Options, ParseOutcome, USAGE, parse_args};
use pulsar_installer::deploy::{self, Station, Ui};
use pulsar_installer::platformtools::find_adb;

fn main() -> ExitCode {
    let code = run();
    pulsar_installer::console::pause_if_own_console();
    ExitCode::from(code)
}

fn run() -> u8 {
    let mut out = io::stdout();
    let mut err = io::stderr();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match parse_args(&args) {
        Ok(ParseOutcome::Help) => {
            let _ = out.write_all(USAGE.as_bytes());
            return 0;
        }
        Ok(ParseOutcome::Run(o)) => o,
        Err(e) => {
            let _ = writeln!(err, "{e}");
            return 1;
        }
    };
    let root = match find_root() {
        Ok(r) => r,
        Err(e) => {
            let _ = writeln!(err, "{e}");
            return 1;
        }
    };
    let Some(adb_path) = find_adb(&root, &mut err) else {
        return 1;
    };
    let mut adb = ProcessAdb::new(adb_path);
    if adb.start_server().is_err() {
        let _ = writeln!(err, "adb start-server failed");
        return 1;
    }
    {
        let mut ui = Ui {
            out: &mut out,
            err: &mut err,
        };
        if !opts.list && !deploy::build(&root, opts.ssh, &mut ui) {
            return 1;
        }
        install(&mut adb, &root, &opts, &mut ui)
    }
}

fn install(adb: &mut ProcessAdb, root: &Path, opts: &Options, ui: &mut Ui) -> u8 {
    ui_line(ui, "== looking for stations");
    let mut seen: Vec<String> = Vec::new();
    let mut found: Vec<Station> = Vec::new();
    let mut targets = opts.targets.clone();
    if targets.is_empty() {
        // Already connected network devices first, then mDNS (or the port scan).
        for s in adb.network_devices() {
            seen.push(s.clone());
            let _ = deploy::probe(adb, &s, false, &mut found, ui);
        }
        targets = if opts.scan {
            pulsar_installer::discover::scan(ui.err)
        } else {
            pulsar_installer::discover::mdns(ui.err)
        };
    }
    for s in &targets {
        if seen.contains(s) {
            continue;
        }
        seen.push(s.clone());
        let ours = if adb.devices().iter().any(|d| d == s) {
            false
        } else {
            adb.connect(s);
            true
        };
        if deploy::probe(adb, s, ours, &mut found, ui) {
            adb.disconnect(s);
        }
    }
    if found.is_empty() {
        ui_line(ui, "no supported rooted station found");
        return 1;
    }
    if opts.list {
        return 0;
    }

    let chosen: Vec<Station> = if opts.yes {
        found.clone()
    } else {
        let ans =
            read_line("install to which? (numbers separated by spaces, a = all, Enter = cancel): ");
        if ans == "a" {
            found.clone()
        } else {
            let mut chosen = Vec::new();
            for n in ans.split_whitespace() {
                let Ok(i) = n.parse::<usize>() else {
                    ui_error(ui, &format!("bad choice: {n}"));
                    return 1;
                };
                if !n.as_bytes()[0].is_ascii_digit() || i < 1 || i > found.len() {
                    ui_error(ui, &format!("bad choice: {n}"));
                    return 1;
                }
                chosen.push(found[i - 1].clone());
            }
            chosen
        }
    };
    if chosen.is_empty() {
        ui_line(ui, "nothing to do");
        return 0;
    }

    let mut failed = false;
    let mut notes = Vec::new();
    let mut prompt = read_line;
    for st in &chosen {
        let (ok, note, lost) = deploy::install_one(adb, root, opts, st, &mut prompt, ui);
        if !ok || lost {
            failed = true;
        }
        if !note.is_empty() {
            notes.push(format!("{}: {note}", st.serial));
        }
    }
    if !notes.is_empty() {
        ui_line(ui, "== autostart summary");
        for n in notes {
            ui_line(ui, &n);
        }
    }
    u8::from(failed)
}

fn ui_line(ui: &mut Ui, s: &str) {
    let _ = writeln!(ui.out, "{s}");
}

fn ui_error(ui: &mut Ui, s: &str) {
    let _ = writeln!(ui.err, "{s}");
}

/// Prints `prompt` on stderr and returns the trimmed answer ("" on EOF).
fn read_line(prompt: &str) -> String {
    let mut err = io::stderr();
    let _ = write!(err, "{prompt}");
    let _ = err.flush();
    let mut line = String::new();
    match io::stdin().read_line(&mut line) {
        Ok(_) => line.trim().to_string(),
        Err(_) => String::new(),
    }
}

/// Locates the payload: next to the executable (release archive), else the working directory
/// or one of its parents (repository checkout).
fn find_root() -> Result<PathBuf, String> {
    let has = |d: &Path| d.join("satellite").join("deploy").join("boot.sh").is_file();
    if let Ok(exe) = std::env::current_exe()
        && let Ok(exe) = exe.canonicalize()
        && let Some(dir) = exe.parent()
        && has(dir)
    {
        return Ok(dir.to_path_buf());
    }
    if let Ok(mut d) = std::env::current_dir() {
        loop {
            if has(&d) {
                return Ok(d);
            }
            let Some(p) = d.parent() else {
                break;
            };
            if p == d {
                break;
            }
            d = p.to_path_buf();
        }
    }
    Err("payload not found: run pulsar-install from the unpacked release archive or inside the repository".to_string())
}
