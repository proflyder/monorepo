//! Command line: flags and the usage text. Behaviour is the Go installer's: unknown
//! options and bad choices exit 1, `-h` prints the full usage on stdout and exits 0.

/// The usage text captured from the Go installer (`testdata/usage.txt`), embedded so the
/// binary and the golden fixture cannot diverge.
pub const USAGE: &str = include_str!("../testdata/usage.txt");

/// The usage the binary prints for `-h`; embedded from the Go fixture.
pub fn usage() -> &'static str {
    USAGE
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Options {
    pub ssh: bool,
    pub no_enable: bool,
    pub yes: bool,
    pub list: bool,
    pub scan: bool,
    pub targets: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseOutcome {
    Run(Options),
    Help,
}

/// Parses argv without the program name, one-to-one with the Go loop: `-h`/`--help` is
/// reported as `Help`, every other option starting with `-` is "unknown option …".
pub fn parse_args(args: &[String]) -> Result<ParseOutcome, String> {
    let mut o = Options::default();
    for a in args {
        match a.as_str() {
            "--ssh" => o.ssh = true,
            "--no-enable" => o.no_enable = true,
            "-y" | "--yes" => o.yes = true,
            "--list" => o.list = true,
            "--scan" => o.scan = true,
            "-h" | "--help" => return Ok(ParseOutcome::Help),
            _ if a.starts_with('-') => return Err(format!("unknown option {a}")),
            _ if a.contains(':') => o.targets.push(a.clone()),
            _ => o.targets.push(format!("{a}:5555")),
        }
    }
    Ok(ParseOutcome::Run(o))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn usage_function_is_the_go_fixture() {
        assert_eq!(usage(), include_str!("../testdata/usage.txt"));
        assert!(usage().starts_with("pulsar-install: finds rooted Yandex Stations"));
    }

    #[test]
    fn parses_flags_and_targets() {
        let ParseOutcome::Run(o) = parse_args(&args(&[
            "--ssh",
            "--no-enable",
            "-y",
            "--list",
            "--scan",
            "192.0.2.10",
            "host:1234",
        ]))
        .unwrap() else {
            panic!("help");
        };
        assert!(o.ssh && o.no_enable && o.yes && o.list && o.scan);
        assert_eq!(o.targets, ["192.0.2.10:5555", "host:1234"]);
    }

    #[test]
    fn help_wins_and_unknown_option_is_exact() {
        assert_eq!(
            parse_args(&args(&["--list", "-h"])).unwrap(),
            ParseOutcome::Help
        );
        assert_eq!(parse_args(&args(&["-x"])).unwrap_err(), "unknown option -x");
        assert_eq!(parse_args(&args(&["--"])).unwrap_err(), "unknown option --");
        assert_eq!(parse_args(&args(&["-"])).unwrap_err(), "unknown option -");
    }
}
