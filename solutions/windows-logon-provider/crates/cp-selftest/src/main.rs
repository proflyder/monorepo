#[allow(clippy::pedantic, clippy::nursery)]
#[cfg(windows)]
mod bindings;
#[cfg(windows)]
mod main_windows;

#[cfg(windows)]
fn main() {
    std::process::exit(main_windows::run());
}

#[cfg(not(windows))]
fn main() {
    eprintln!("cp-selftest is a Windows-only tool");
    std::process::exit(2);
}
