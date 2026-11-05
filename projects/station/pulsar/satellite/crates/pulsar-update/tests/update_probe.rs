//! `update_probe` против C++ пробника: tests/golden/versions.txt — stdout C++
//! `update_probe versions` (тег rust-port-base, сборка хоста zig c++).

use std::process::Command;

fn probe(args: &[&str]) -> std::process::Output {
    // Cargo задаёт CARGO_BIN_EXE_*; Bazel — только через env теста (см. BUILD.bazel).
    let binary = std::env::var("CARGO_BIN_EXE_update_probe").expect("CARGO_BIN_EXE_update_probe");
    Command::new(binary)
        .args(args)
        .output()
        .expect("run update_probe")
}

#[test]
fn versions_table_matches_the_cxx_probe() {
    let out = probe(&["versions"]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        include_str!("golden/versions.txt")
    );
}

#[test]
fn usage() {
    for args in [
        &[][..],
        &["download"],
        &["check", "a", "b"],
        &["install"],
        &["bogus"],
    ] {
        let out = probe(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("Usage:"), "{err}");
    }
}
