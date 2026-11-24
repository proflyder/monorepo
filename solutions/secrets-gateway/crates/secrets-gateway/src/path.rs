use thiserror::Error;

const MAX_PATH_CHARS: usize = 256;
const MAX_SEGMENTS: usize = 8;
const MOUNT_PREFIXES: [&str; 3] = ["secret/data/", "secret/metadata/", "secret/"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PathError {
    #[error("path is empty")]
    Empty,
    #[error("path is too long")]
    TooLong,
    #[error("path has too many segments")]
    TooManySegments,
    #[error("path has an invalid segment")]
    InvalidSegment,
}

pub fn normalize(raw: &str) -> Result<String, PathError> {
    let trimmed = raw.trim().trim_matches('/');
    let mut value = trimmed;
    for prefix in MOUNT_PREFIXES {
        if let Some(stripped) = trimmed.strip_prefix(prefix) {
            value = stripped.trim_matches('/');
            break;
        }
    }
    if value.is_empty() {
        return Err(PathError::Empty);
    }
    if value.chars().count() > MAX_PATH_CHARS {
        return Err(PathError::TooLong);
    }
    if value.split('/').count() > MAX_SEGMENTS {
        return Err(PathError::TooManySegments);
    }
    if !value.split('/').all(valid_segment) {
        return Err(PathError::InvalidSegment);
    }
    Ok(value.to_owned())
}

fn valid_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '+' | '@')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mount_prefix_is_removed_and_paths_are_trimmed() {
        assert_eq!(normalize("  /k8s/test/  ").expect("path"), "k8s/test");
        assert_eq!(normalize("secret/data/k8s/test").expect("path"), "k8s/test");
        assert_eq!(
            normalize("secret/metadata/k8s/test").expect("path"),
            "k8s/test"
        );
        assert_eq!(normalize("secret/k8s/test").expect("path"), "k8s/test");
    }

    #[test]
    fn traversal_empty_and_unsafe_segments_are_rejected() {
        for raw in [
            "",
            "   ",
            "/",
            "../sys/seal",
            "k8s/../sys/seal",
            "k8s//test",
            "k8s/test\u{0}",
            "k8s/test?version=1",
            "k8s/тест",
        ] {
            assert!(normalize(raw).is_err(), "{raw:?}");
        }
        assert_eq!(
            normalize(&"a/".repeat(MAX_SEGMENTS + 1)).expect_err("segments"),
            PathError::TooManySegments
        );
        assert_eq!(
            normalize(&"a".repeat(MAX_PATH_CHARS + 1)).expect_err("length"),
            PathError::TooLong
        );
    }
}
