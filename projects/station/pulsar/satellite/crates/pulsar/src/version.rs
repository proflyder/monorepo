//! Set by the release build: the VERSION file and HEAD's commit date (`--define=pulsar_version`,
//! `--define=pulsar_build_time` via satellite/version.bzl); defaults are "unknown".

/// `pulsar --version`, Home Assistant DeviceInfo and the "Version" entity; the updater compares
/// it with the release tag.
pub const VERSION: &str = match option_env!("PULSAR_VERSION") {
    Some(v) => v,
    None => "unknown",
};

/// "Mmm dd yyyy, hh:mm:ss" (C++ `__DATE__ ", " __TIME__`), HA DeviceInfo compilation_time.
pub const BUILD_TIME: &str = match option_env!("PULSAR_BUILD_TIME") {
    Some(v) => v,
    None => "unknown",
};
