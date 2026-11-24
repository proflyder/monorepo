use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
struct DenyConfig {
    #[serde(default)]
    prefixes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Deny {
    prefixes: Vec<String>,
}

impl Deny {
    pub fn load(path: &Path) -> Result<Self, String> {
        let data = std::fs::read_to_string(path)
            .map_err(|error| format!("deny {}: {error}", path.display()))?;
        Self::parse(&data)
    }

    pub fn parse(json: &str) -> Result<Self, String> {
        let config: DenyConfig =
            serde_json::from_str(json).map_err(|error| format!("deny JSON: {error}"))?;
        let mut prefixes = Vec::new();
        for prefix in config.prefixes {
            let trimmed = prefix.trim();
            if trimmed.is_empty()
                || trimmed.starts_with('/')
                || trimmed.ends_with('/')
                || trimmed.contains("//")
                || trimmed
                    .split('/')
                    .any(|segment| segment == "." || segment == "..")
                || !trimmed
                    .split('/')
                    .all(|segment| !segment.is_empty() && segment.is_ascii())
            {
                return Err(format!("invalid deny prefix: {prefix:?}"));
            }
            prefixes.push(trimmed.to_owned());
        }
        Ok(Self { prefixes })
    }

    #[must_use]
    pub fn is_denied(&self, path: &str) -> bool {
        let lowered = path.to_ascii_lowercase();
        self.prefixes
            .iter()
            .any(|prefix| prefix_matches(&lowered, prefix))
    }

    #[must_use]
    pub fn prefixes(&self) -> &[String] {
        &self.prefixes
    }
}

fn prefix_matches(path: &str, prefix: &str) -> bool {
    if path == prefix {
        return true;
    }
    path.strip_prefix(prefix)
        .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORE: &str = r#"{"prefixes":[
        "openbao",
        "infra/vpn/client-ca",
        "k8s/flux-system",
        "k8s/home-assistant/scheduler-auth",
        "k8s/home-assistant/secrets-gateway"
    ]}"#;

    fn deny() -> Deny {
        Deny::parse(CORE).expect("deny")
    }

    #[test]
    fn only_the_core_is_denied() {
        for path in [
            "openbao",
            "openbao/seal",
            "infra/vpn/client-ca",
            "infra/vpn/client-ca/ca.key",
            "k8s/flux-system",
            "k8s/flux-system/git-deploy-key",
            "k8s/home-assistant/scheduler-auth",
            "k8s/home-assistant/secrets-gateway",
        ] {
            assert!(deny().is_denied(path), "{path}");
        }
        for path in [
            "k8s/test",
            "k8s/home-assistant/phone-push",
            "infra/vpn/owner",
            "infra/backup/offsite",
            "ci/test",
            "k8s/home-assistant/secrets-gateway-state",
        ] {
            assert!(!deny().is_denied(path), "{path}");
        }
    }

    #[test]
    fn a_prefix_only_matches_whole_segments() {
        let deny = deny();
        assert!(!deny.is_denied("openbao2/test"));
        assert!(!deny.is_denied("infra/vpn/client-ca2"));
        assert!(deny.is_denied("infra/vpn/client-ca"));
    }

    #[test]
    fn invalid_prefixes_are_rejected() {
        for json in [
            r#"{"prefixes":[""]}"#,
            r#"{"prefixes":["/absolute"]}"#,
            r#"{"prefixes":["trailing/"]}"#,
            r#"{"prefixes":["a//b"]}"#,
            r#"{"prefixes":["a/../b"]}"#,
            r#"{"prefixes":["кириллица"]}"#,
        ] {
            assert!(Deny::parse(json).is_err(), "{json}");
        }
    }
}
