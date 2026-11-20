use std::env;
use std::path::PathBuf;

const LIBS: [(&str, &str); 6] = [
    ("sendspin", "sendspin"),
    ("sendspin", "noise_c"),
    ("_deps/micro_flac-build", "micro_flac"),
    ("_deps/micro_opus-build", "micro_opus"),
    (
        "_deps/micro_opus-build/micro-ogg-demuxer",
        "micro_ogg_demuxer",
    ),
    ("_deps/ixwebsocket-build", "ixwebsocket"),
];

const ROLES: [&str; 5] = [
    "SENDSPIN_ENABLE_PLAYER",
    "SENDSPIN_ENABLE_OPUS",
    "SENDSPIN_ENABLE_METADATA",
    "SENDSPIN_ENABLE_CONTROLLER",
    "SENDSPIN_ENABLE_VISUALIZER",
];

fn main() {
    println!("cargo:rustc-check-cfg=cfg(sendspin_ffi)");
    for var in ["PULSAR_SS_BUILD", "PULSAR_SS_SRC", "PULSAR_REQUIRE_SS"] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    println!("cargo:rerun-if-changed=shim/sendspin_shim.h");
    println!("cargo:rerun-if-changed=shim/sendspin_shim.cpp");

    let Some(build) = env::var_os("PULSAR_SS_BUILD").map(PathBuf::from) else {
        if env::var_os("PULSAR_REQUIRE_SS").is_some_and(|v| v == "1") {
            panic!("PULSAR_SS_BUILD is not set: build via make (it builds sendspin-cpp first)");
        }
        return;
    };
    let src = env::var_os("PULSAR_SS_SRC")
        .map(PathBuf::from)
        .expect("PULSAR_SS_SRC is not set: build via make");

    let mut shim = cc::Build::new();
    shim.cpp(true)
        .std("c++20")
        .include(src.join("sendspin-cpp/include"))
        .include(src.join("noise-c/include"))
        .file("shim/sendspin_shim.cpp");
    for role in ROLES {
        shim.define(role, None);
    }
    shim.compile("sendspin_shim");

    for (dir, lib) in LIBS {
        let dir = build.join(dir);
        println!(
            "cargo:rerun-if-changed={}",
            dir.join(format!("lib{lib}.a")).display()
        );
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-lib=static={lib}");
    }
    println!("cargo:rustc-link-lib=c++");
    println!("cargo:rustc-cfg=sendspin_ffi");
}
