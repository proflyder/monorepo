//! HMAC-SHA256 message shapes and constant-time comparison.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

pub fn hmac_sha256_b64(key: &[u8], message: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(message.as_bytes());
    crate::b64_encode(&mac.finalize().into_bytes())
}

/// Length-independent comparison; both inputs are compared in full.
pub fn constant_time_equal<A: AsRef<[u8]>, B: AsRef<[u8]>>(a: A, b: B) -> bool {
    let a = a.as_ref();
    let b = b.as_ref();
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

/// `<kind>\n<machine>\n<nonce>` with `kind = "claim"`.
pub fn claim_mac_message(machine: &str, nonce: &str) -> String {
    format!("claim\n{machine}\n{nonce}")
}

/// `<kind>\n<machine>\n<nonce>\n<granted 0|1>\n<grantId>` with `kind = "grant"`.
pub fn grant_mac_message(machine: &str, nonce: &str, granted: bool, grant_id: &str) -> String {
    let flag = if granted { '1' } else { '0' };
    format!("grant\n{machine}\n{nonce}\n{flag}\n{grant_id}")
}
