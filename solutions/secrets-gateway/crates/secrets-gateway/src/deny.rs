pub const DENIED_PREFIXES: &[&str] = &[
    "auth",
    "ci",
    "delete",
    "destroy",
    "identity",
    "infra",
    "k8s/flux-system",
    "k8s/home-assistant/scheduler-auth",
    "k8s/home-assistant/secrets-gateway",
    "openbao",
    "subkeys",
    "sys",
    "undelete",
];

pub const DENIED_SEGMENTS: &[&str] = &[
    "operator",
    "recovery",
    "recovery-key",
    "seal",
    "unseal",
    "unseal-key",
];

pub fn is_denied(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    if DENIED_PREFIXES
        .iter()
        .any(|prefix| prefix_matches(&lowered, prefix))
    {
        return true;
    }
    lowered
        .split('/')
        .any(|segment| DENIED_SEGMENTS.contains(&segment))
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

    #[test]
    fn owner_infrastructure_and_openbao_internals_are_denied() {
        for path in [
            "infra/vpn/client-ca",
            "infra/vpn/owner",
            "infra/kubernetes/admin",
            "infra/backup/offsite",
            "infra",
            "ci/android-distribute",
            "ci/test",
            "k8s/flux-system/git-deploy-key",
            "k8s/home-assistant/scheduler-auth",
            "k8s/home-assistant/secrets-gateway",
            "openbao/seal",
            "sys/seal",
            "sys/raw/core",
            "auth/kubernetes/role/secrets-gateway",
            "identity/entity/id",
            "k8s/anything/unseal",
            "k8s/anything/unseal-key",
            "destroy/k8s/test",
            "delete/k8s/test",
            "undelete/k8s/test",
            "subkeys/k8s/test",
        ] {
            assert!(is_denied(path), "{path}");
        }
    }

    #[test]
    fn application_secrets_remain_available() {
        for path in [
            "k8s/test",
            "k8s/home-assistant/phone-push",
            "k8s/home-assistant/hermes-environment",
            "k8s/vaultwarden/vaultwarden-admin",
        ] {
            assert!(!is_denied(path), "{path}");
        }
    }

    #[test]
    fn a_prefix_only_matches_whole_segments() {
        assert!(!is_denied("infrastructure/test"));
        assert!(!is_denied("cinema/test"));
        assert!(is_denied("infra/test"));
        assert!(is_denied("ci/test"));
    }
}
