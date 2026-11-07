//! Short deploy build identifiers.
//!
//! The deploy scripts used to inline a Python SHA-256 expression; this module
//! owns the fingerprint half so every unit derives the same id from the same
//! image set. It matches `platform/deploy/internal/buildid`.

use sha2::{Digest, Sha256};

/// Number of hex characters kept from the digest.
pub const SHORT_LENGTH: usize = 12;

/// Returns the first 12 hex characters of `sha256(fingerprint)`.
pub fn fingerprint(fingerprint: &str) -> String {
    let digest = Sha256::digest(fingerprint.as_bytes());
    hex::encode(digest)[..SHORT_LENGTH].to_string()
}

/// Joins the short git revision and the fingerprint digest.
pub fn build_id(revision: &str, fingerprint: &str) -> String {
    format!("{revision}-{}", self::fingerprint(fingerprint))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_the_go_vectors() {
        // Generated with the Go implementation and the Python expression it
        // replaced (sha256("hello") = 2cf24dba5fb0...).
        assert_eq!(fingerprint("hello"), "2cf24dba5fb0");
        assert_eq!(build_id("abc123", "hello"), "abc123-2cf24dba5fb0");
        assert_eq!(fingerprint(""), "e3b0c44298fc");
    }
}
