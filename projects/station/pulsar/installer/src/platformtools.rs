//! Android SDK Platform-Tools: find `adb`, or download and unpack Google's archive.
//!
//! Google's license forbids redistributing platform-tools, so they are downloaded on first
//! use. Pins are from https://dl.google.com/android/repository/repository2-3.xml.

use std::ffi::OsString;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use sha1::{Digest, Sha1};

pub const PT_BASE: &str = "https://dl.google.com/android/repository/";

#[derive(Debug, Clone, Copy)]
pub struct PtPackage {
    pub file: &'static str,
    pub sha1: &'static str,
    pub size: u64,
}

/// GOOS -> package; linux only on amd64 (Google builds no other).
pub fn package_for(os: &str, arch: &str) -> Option<PtPackage> {
    match os {
        "windows" => Some(PtPackage {
            file: "platform-tools_r37.0.1-win.zip",
            sha1: "e03e78b1d80b396f1c3358e31251cb31740e1110",
            size: 8_044_989,
        }),
        "macos" | "darwin" => Some(PtPackage {
            file: "platform-tools_r37.0.1-darwin.zip",
            sha1: "6ae73f4de6452dc57e62ec02b68eed92a4c21661",
            size: 16_110_554,
        }),
        "linux" if arch == "x86_64" => Some(PtPackage {
            file: "platform-tools_r37.0.1-linux.zip",
            sha1: "477254aa5f903c15cf51001717bdf347fb6b53e0",
            size: 9_054_187,
        }),
        _ => None,
    }
}

fn adb_exe() -> &'static str {
    if cfg!(windows) { "adb.exe" } else { "adb" }
}

fn is_executable(p: &Path) -> bool {
    let Ok(md) = p.metadata() else {
        return false;
    };
    if !md.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        md.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// `bazel`, then `bazelisk`: the repository only guarantees the mise shim.
pub fn bazel_tool() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| bazel_tool_in(&path))
}

pub fn bazel_tool_in(path: &std::ffi::OsStr) -> Option<PathBuf> {
    look_path_in(path, "bazel").or_else(|| look_path_in(path, "bazelisk"))
}

fn look_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    look_path_in(&path, name)
}

fn look_path_in(path: &std::ffi::OsStr, name: &str) -> Option<PathBuf> {
    // Go's exec.LookPath: "." and empty entries do not search the current directory.
    let dirs =
        std::env::split_paths(&path).filter(|d| !d.as_os_str().is_empty() && d.as_os_str() != ".");
    #[cfg(windows)]
    let extensions: Vec<String> = {
        let mut v = Vec::new();
        if let Ok(pathext) = std::env::var("PATHEXT") {
            v.extend(pathext.split(';').map(|e| e.trim().to_string()));
        } else {
            v.extend([".COM", ".EXE", ".BAT", ".CMD"].map(str::to_string));
        }
        v
    };
    for dir in dirs {
        #[cfg(windows)]
        {
            if Path::new(name).extension().is_none() {
                for ext in &extensions {
                    let p = dir.join(format!("{name}{ext}"));
                    if is_executable(&p) {
                        return Some(p);
                    }
                }
                continue;
            }
            let p = dir.join(name);
            if is_executable(&p) {
                return Some(p);
            }
        }
        #[cfg(not(windows))]
        {
            let p = dir.join(name);
            if is_executable(&p) {
                return Some(p);
            }
        }
    }
    None
}

/// Sets the adb path: `adb` from PATH, else `platform-tools/` next to the payload, else
/// downloaded there.
pub fn find_adb(root: &Path, err: &mut dyn Write) -> Option<PathBuf> {
    if let Some(p) = look_path("adb") {
        return Some(p);
    }
    let dest = root.join("platform-tools");
    let local = dest.join(adb_exe());
    if local.is_file() {
        return Some(local);
    }
    let Some(pkg) = package_for(std::env::consts::OS, std::env::consts::ARCH) else {
        let _ = writeln!(
            err,
            "adb not found (Android platform-tools): install it, e.g. sudo apt install adb"
        );
        return None;
    };
    let url = format!("{PT_BASE}{}", pkg.file);
    let _ = writeln!(
        err,
        "adb not found: downloading Android SDK Platform-Tools 37.0.1 from {url} (terms: https://developer.android.com/studio/terms)"
    );
    if let Err(e) = fetch_platform_tools(&url, pkg.size, pkg.sha1, &dest) {
        let _ = writeln!(
            err,
            "platform-tools: {e}; install adb yourself (Android platform-tools)"
        );
        return None;
    }
    Some(local)
}

/// Downloads `url`, checks size and SHA-1 and unpacks the zip's `platform-tools/` tree into
/// `dest.tmp-<pid>`, then renames that to `dest`. On any error nothing is left behind.
pub fn fetch_platform_tools(
    url: &str,
    size: u64,
    sha1_hex: &str,
    dest: &Path,
) -> Result<(), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5 * 60)))
        .build()
        .into();
    let response = agent.get(url).call().map_err(|e| match e {
        ureq::Error::StatusCode(code) => http_status(code),
        other => other.to_string(),
    })?;
    let mut data = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(size + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    let got = hex::encode(Sha1::digest(&data));
    if data.len() as u64 != size || got != sha1_hex {
        return Err(format!("checksum mismatch (got {got})"));
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(data)).map_err(|e| e.to_string())?;
    let tmp = tmp_name(dest);
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp).map_err(|e| e.to_string())?;
    }
    if let Err(e) = unpack(&mut archive, &tmp) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, dest) {
        let _ = std::fs::remove_dir_all(&tmp);
        if dest.exists() {
            return Ok(()); // another run got there first
        }
        return Err(e.to_string());
    }
    Ok(())
}

fn tmp_name(dest: &Path) -> PathBuf {
    let mut name: OsString = dest.as_os_str().to_owned();
    name.push(format!(".tmp-{}", std::process::id()));
    PathBuf::from(name)
}

fn http_status(code: u16) -> String {
    http::StatusCode::from_u16(code).map_or_else(
        |_| format!("HTTP {code}"),
        |s| {
            s.canonical_reason().map_or_else(
                || format!("HTTP {code}"),
                |reason| format!("HTTP {code} {reason}"),
            )
        },
    )
}

/// `filepath.IsLocal` after `filepath.Join` cleaning: a relative path whose `..` never
/// escapes its base. `a/../b` is local, `../b` is not.
fn clean_local(p: &Path) -> Option<PathBuf> {
    if p.is_absolute() {
        return None;
    }
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(out)
}

fn unpack(archive: &mut zip::ZipArchive<Cursor<Vec<u8>>>, dir: &Path) -> Result<(), String> {
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().map_err(|e| e.to_string())?.to_string();
        let Some(rest) = name.strip_prefix("platform-tools/") else {
            return Err(format!("unexpected entry {name}"));
        };
        let rest_path = Path::new(rest);
        let Some(clean_rest) = clean_local(rest_path) else {
            return Err(format!("unexpected entry {name}"));
        };
        let out = dir.join(&clean_rest);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else if entry.is_symlink() {
            let mut target = String::new();
            entry
                .by_ref()
                .take(4096)
                .read_to_string(&mut target)
                .map_err(|e| e.to_string())?;
            let target_path = Path::new(&target);
            let joined = rest_path
                .parent()
                .unwrap_or(Path::new(""))
                .join(target_path);
            if clean_local(&joined).is_none() {
                return Err(format!("unexpected entry {name} -> {target}"));
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(target_path, &out).map_err(|e| e.to_string())?;
            #[cfg(not(unix))]
            return Err(format!("unexpected entry {name} -> {target}"));
        } else {
            if clean_rest.as_os_str().is_empty() {
                return Err(format!("unexpected entry {name}"));
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            // Perm(): only the low nine bits; setuid/sticky are not kept. The mode goes to
            // O_CREAT, so the process umask applies exactly as it did in Go.
            let perm = (entry.unix_mode().unwrap_or(0o644) & 0o777) | 0o600;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(perm);
            }
            #[cfg(not(unix))]
            let _ = perm;
            let mut file = options.open(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::thread;

    #[cfg(not(windows))]
    #[test]
    fn bazel_tool_prefers_bazel_then_bazelisk() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path();
        let path = bin.as_os_str();
        assert!(bazel_tool_in(path).is_none());
        let bazelisk = bin.join("bazelisk");
        std::fs::write(&bazelisk, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bazelisk, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(bazel_tool_in(path), Some(bazelisk));
        let bazel = bin.join("bazel");
        std::fs::write(&bazel, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bazel, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(bazel_tool_in(path), Some(bazel));
    }

    #[test]
    fn http_status_keeps_go_reason_phrase() {
        assert_eq!(http_status(404), "HTTP 404 Not Found");
        assert_eq!(http_status(599), "HTTP 599");
    }

    #[test]
    fn local_path_cleaning_matches_filepath_join() {
        assert_eq!(clean_local(Path::new("a/../b")), Some(PathBuf::from("b")));
        assert_eq!(clean_local(Path::new("../b")), None);
        assert_eq!(clean_local(Path::new("/etc/passwd")), None);
        assert_eq!(clean_local(Path::new("")), Some(PathBuf::new()));
    }

    #[test]
    fn pins_match_go_golden() {
        let f: Value =
            serde_json::from_str(include_str!("../testdata/platformtools.json")).unwrap();
        for (os, entry) in f.as_object().unwrap() {
            let arch = if os == "linux" { "x86_64" } else { "aarch64" };
            let pkg = package_for(os, arch).unwrap();
            assert_eq!(pkg.file, entry["file"].as_str().unwrap());
            assert_eq!(pkg.sha1, entry["sha1"].as_str().unwrap());
            assert_eq!(pkg.size, entry["size"].as_u64().unwrap());
        }
        assert!(package_for("linux", "aarch64").is_none());
    }

    fn zip_of(entry: &str) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o755);
            zw.start_file(entry, opts).unwrap();
            zw.write_all(b"x").unwrap();
            zw.finish().unwrap();
        }
        buf.into_inner()
    }

    /// Minimal one-shot HTTP server; returns the URL and serves `body` once.
    fn serve(body: Vec<u8>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
        });
        (format!("http://{addr}/platform-tools.zip"), handle)
    }

    fn sha1_hex(body: &[u8]) -> String {
        hex::encode(Sha1::digest(body))
    }

    #[test]
    fn fetch_unpacks_and_checks() {
        let body = zip_of("platform-tools/adb");
        let (url, handle) = serve(body.clone());
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("platform-tools");
        fetch_platform_tools(&url, body.len() as u64, &sha1_hex(&body), &dest).unwrap();
        handle.join().unwrap();
        let st = std::fs::metadata(dest.join("adb")).unwrap();
        assert!(st.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(st.permissions().mode() & 0o100, 0);
        }
        let left: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["platform-tools"]);
    }

    #[test]
    fn fetch_rejects_bad_checksum() {
        let body = zip_of("platform-tools/adb");
        let (url, handle) = serve(body.clone());
        let dir = tempfile::tempdir().unwrap();
        let err = fetch_platform_tools(
            &url,
            body.len() as u64,
            "0000000000000000000000000000000000000000",
            &dir.path().join("platform-tools"),
        )
        .unwrap_err();
        handle.join().unwrap();
        assert!(err.contains("checksum mismatch"));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn fetch_rejects_escape() {
        let body = zip_of("platform-tools/../evil");
        let (url, handle) = serve(body.clone());
        let dir = tempfile::tempdir().unwrap();
        let err = fetch_platform_tools(
            &url,
            body.len() as u64,
            &sha1_hex(&body),
            &dir.path().join("sub").join("platform-tools"),
        )
        .unwrap_err();
        handle.join().unwrap();
        assert!(err.contains("unexpected entry"));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
