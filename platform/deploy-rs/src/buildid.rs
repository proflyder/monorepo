use sha2::{Digest, Sha256};

pub const SHORT_LENGTH: usize = 12;

pub fn fingerprint(fingerprint: &str) -> String {
    let digest = Sha256::digest(fingerprint.as_bytes());
    hex::encode(digest)[..SHORT_LENGTH].to_string()
}

pub fn build_id(revision: &str, fingerprint: &str) -> String {
    format!("{revision}-{}", self::fingerprint(fingerprint))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_the_go_vectors() {
        assert_eq!(fingerprint("hello"), "2cf24dba5fb0");
        assert_eq!(build_id("abc123", "hello"), "abc123-2cf24dba5fb0");
        assert_eq!(fingerprint(""), "e3b0c44298fc");
    }
}
