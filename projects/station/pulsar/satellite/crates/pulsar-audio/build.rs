use std::env;
use std::path::{Path, PathBuf};

fn collect_cc(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_cc(&path, out);
        } else if path.extension().is_some_and(|e| e == "cc") {
            out.push(path);
        }
    }
}

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    println!("cargo:rerun-if-changed=ricardo");
    println!("cargo:rerun-if-changed=build.rs");

    let manifest = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo and rules_rust"),
    );

    let mut sources = vec![manifest.join("ricardo/microwakeword.cpp")];
    collect_cc(&manifest.join("ricardo/microfrontend"), &mut sources);
    sources.sort();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .include(manifest.join("ricardo/microfrontend"))
        .include(manifest.join("ricardo/microfrontend/kissfft"))
        .include(manifest.join("ricardo/microfrontend/kissfft/tools"))
        .warnings(false);
    for source in sources {
        build.file(source);
    }
    build.compile("ricardo_mww");

    if target.contains("linux") {
        println!("cargo:rustc-link-lib=dl");
        println!("cargo:rustc-link-lib=m");
        if env::var_os("CARGO_ZIGBUILD_ZIG_PATH").is_some() {
            println!("cargo:rustc-link-lib=c++");
        }
    }
}
