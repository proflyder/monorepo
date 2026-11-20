pub const VERSION: &str = match option_env!("PULSAR_VERSION") {
    Some(v) => v,
    None => "unknown",
};

pub const BUILD_TIME: &str = match option_env!("PULSAR_BUILD_TIME") {
    Some(v) => v,
    None => "unknown",
};
