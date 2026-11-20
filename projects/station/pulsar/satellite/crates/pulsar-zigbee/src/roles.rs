#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Off,
    Coordinator,
    Router,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Role {
    pub mode: Mode,
    pub name: &'static str,
    pub label: &'static str,
    pub family: u8,
}

pub const ROLES: [Role; 3] = [
    Role {
        mode: Mode::Off,
        name: "off",
        label: "Off",
        family: 0,
    },
    Role {
        mode: Mode::Coordinator,
        name: "pulsar-coordinator",
        label: "Coordinator",
        family: 0x51,
    },
    Role {
        mode: Mode::Router,
        name: "router",
        label: "Router",
        family: 0x50,
    },
];

static TABLE: [Role; 3] = ROLES;

pub fn role(m: Mode) -> &'static Role {
    TABLE.iter().find(|r| r.mode == m).unwrap_or(&TABLE[0])
}

pub fn parse_mode(s: &str) -> Mode {
    TABLE
        .iter()
        .find(|r| r.name == s)
        .map_or(Mode::Off, |r| r.mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_modes_and_families() {
        assert_eq!(parse_mode("off"), Mode::Off);
        assert_eq!(parse_mode("pulsar-coordinator"), Mode::Coordinator);
        assert_eq!(parse_mode("router"), Mode::Router);
        for other in ["coordinator", "", "Router", "router ", "on"] {
            assert_eq!(parse_mode(other), Mode::Off, "{other:?}");
        }
        assert_eq!(role(Mode::Off).label, "Off");
        assert_eq!(role(Mode::Coordinator).family, 0x51);
        assert_eq!(role(Mode::Coordinator).label, "Coordinator");
        assert_eq!(role(Mode::Router).family, 0x50);
        assert_eq!(role(Mode::Router).name, "router");
    }
}
