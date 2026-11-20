use unicode_normalization::UnicodeNormalization as _;

use super::sanitize::collapse_space;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Profile {
    Work,
    Personal,
}

impl Profile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Personal => "personal",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "work" => Some(Self::Work),
            "personal" => Some(Self::Personal),
            _ => None,
        }
    }
}

#[must_use]
pub fn map_state(state: &str) -> Option<Profile> {
    match normalize_state(state).as_str() {
        "work" | "рабочий" | "рабочий профиль" => Some(Profile::Work),
        "personal" | "личный" | "личный профиль" => Some(Profile::Personal),
        _ => None,
    }
}

#[must_use]
pub fn normalize_state(state: &str) -> String {
    let normalized: String = state.nfkc().collect();
    let folded = caseless::default_case_fold_str(&normalized);
    collapse_space(&folded)
}

#[cfg(test)]
mod tests {
    use super::{Profile, map_state, normalize_state};

    #[test]
    fn known_states_map_to_profiles() {
        assert_eq!(map_state("Рабочий"), Some(Profile::Work));
        assert_eq!(map_state("рабочий профиль"), Some(Profile::Work));
        assert_eq!(map_state("WORK"), Some(Profile::Work));
        assert_eq!(map_state("Личный"), Some(Profile::Personal));
        assert_eq!(map_state("личный профиль"), Some(Profile::Personal));
        assert_eq!(map_state("personal"), Some(Profile::Personal));
    }

    #[test]
    fn unknown_and_truncated_states_are_rejected() {
        assert_eq!(map_state(""), None);
        assert_eq!(map_state("рабочий режим"), None);
        assert_eq!(map_state("unknown"), None);
    }

    #[test]
    fn normalisation_handles_fullwidth_and_nbsp() {
        assert_eq!(normalize_state("ＷＯＲＫ"), "work");
        assert_eq!(normalize_state("рабочий\u{00a0}профиль"), "рабочий профиль");
    }
}
