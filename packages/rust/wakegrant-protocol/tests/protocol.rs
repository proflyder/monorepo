//! Protocol vectors and fail-closed branches from `PROTOCOL.md`.

use std::fmt::Write as _;

use wakegrant_protocol::{
    ClaimOutcome, Purpose, b64_decode, b64_encode, challenge_request_json, claim_mac_message,
    claim_mac_message_for_purpose, claim_request_json, claim_request_json_for_purpose,
    constant_time_equal, grant_mac_message, grant_mac_message_for_purpose, hmac_sha256_b64,
    parse_challenge_response, parse_secret_json, verify_claim_response,
    verify_claim_response_for_purpose,
};

fn byte_range(first: u8, last: u8) -> Vec<u8> {
    (first..=last).collect()
}

fn make_claim_body(granted: bool, grant_id: &str, mac: &str) -> String {
    let mut body = format!("{{\"granted\":{granted}");
    if !grant_id.is_empty() {
        let _ = write!(body, ",\"grantId\":\"{grant_id}\"");
    }
    let _ = write!(body, ",\"mac\":\"{mac}\"}}");
    body
}

struct TestVector {
    key: Vec<u8>,
    machine: &'static str,
    nonce: &'static str,
    grant_id: &'static str,
}

impl TestVector {
    fn new() -> Self {
        Self {
            key: byte_range(0x00, 0x1f),
            machine: "komp",
            nonce: "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
            grant_id: "00000000-0000-4000-8000-000000000001",
        }
    }
}

#[test]
fn protocol_vectors() {
    let vector = TestVector::new();

    assert_eq!(
        claim_mac_message(vector.machine, vector.nonce),
        format!("claim\nkomp\n{}", vector.nonce)
    );
    assert_eq!(
        grant_mac_message(vector.machine, vector.nonce, true, vector.grant_id),
        format!("grant\nkomp\n{}\n1\n{}", vector.nonce, vector.grant_id)
    );
    assert_eq!(
        grant_mac_message(vector.machine, vector.nonce, false, ""),
        format!("grant\nkomp\n{}\n0\n", vector.nonce)
    );

    assert_eq!(
        hmac_sha256_b64(
            &vector.key,
            &claim_mac_message(vector.machine, vector.nonce)
        ),
        "+YedSn/aeRZqRNKKzkUS8Dy2aq/dVLkXbh9iX91L9XU="
    );
    assert_eq!(
        hmac_sha256_b64(
            &vector.key,
            &grant_mac_message(vector.machine, vector.nonce, true, vector.grant_id)
        ),
        "UAADAs7pUxn/EN42woIH5K0qrH2YB+bd1SYZVKP//FI="
    );
    assert_eq!(
        hmac_sha256_b64(
            &vector.key,
            &grant_mac_message(vector.machine, vector.nonce, false, "")
        ),
        "I6ClmYKmmfO6wodeKXOobAtJj/SgQ7C/ieNumDYrj1Y="
    );

    let granted_mac = "UAADAs7pUxn/EN42woIH5K0qrH2YB+bd1SYZVKP//FI=";
    let granted = verify_claim_response(
        &make_claim_body(true, vector.grant_id, granted_mac),
        &vector.key,
        vector.machine,
        vector.nonce,
    );
    assert_eq!(granted.outcome, ClaimOutcome::Granted);
    assert_eq!(granted.grant_id, vector.grant_id);

    let denied = verify_claim_response(
        &make_claim_body(false, "", "I6ClmYKmmfO6wodeKXOobAtJj/SgQ7C/ieNumDYrj1Y="),
        &vector.key,
        vector.machine,
        vector.nonce,
    );
    assert_eq!(denied.outcome, ClaimOutcome::Denied);
    assert!(denied.grant_id.is_empty());

    assert_eq!(
        verify_claim_response(
            &make_claim_body(
                true,
                vector.grant_id,
                "I6ClmYKmmfO6wodeKXOobAtJj/SgQ7C/ieNumDYrj1Y="
            ),
            &vector.key,
            vector.machine,
            vector.nonce,
        )
        .outcome,
        ClaimOutcome::InvalidMac
    );

    assert_eq!(
        verify_claim_response(
            &make_claim_body(false, "", granted_mac),
            &vector.key,
            vector.machine,
            vector.nonce,
        )
        .outcome,
        ClaimOutcome::InvalidMac
    );

    // Replay against another nonce, another machine or another key is rejected.
    assert_eq!(
        verify_claim_response(
            &make_claim_body(true, vector.grant_id, granted_mac),
            &vector.key,
            vector.machine,
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHg8=",
        )
        .outcome,
        ClaimOutcome::InvalidMac
    );
    assert_eq!(
        verify_claim_response(
            &make_claim_body(true, vector.grant_id, granted_mac),
            &vector.key,
            "other",
            vector.nonce,
        )
        .outcome,
        ClaimOutcome::InvalidMac
    );
    let mut other_key = vector.key.clone();
    other_key[0] ^= 0x01;
    assert_eq!(
        verify_claim_response(
            &make_claim_body(true, vector.grant_id, granted_mac),
            &other_key,
            vector.machine,
            vector.nonce,
        )
        .outcome,
        ClaimOutcome::InvalidMac
    );
    assert_eq!(
        verify_claim_response(
            &make_claim_body(true, "00000000-0000-4000-8000-000000000002", granted_mac),
            &vector.key,
            vector.machine,
            vector.nonce,
        )
        .outcome,
        ClaimOutcome::InvalidMac
    );
}

/// The purpose-aware vectors pinned in `PROTOCOL.md`, byte for byte.
#[test]
fn purpose_aware_protocol_vectors() {
    let vector = TestVector::new();

    let expected = [
        (
            Purpose::Logon,
            "AMPMcTPW1kuBTUp8sWtF4weIoI/Vtcp0R+cNBK+xdB0=",
            "kmFSwXXu4g4wylTSIf9DSYht1v28pG4jboomTnocI0k=",
            "HkwjpcROboJ+1uR7P8Rydoo2JvNFHx8iOEI0BS0G7K8=",
        ),
        (
            Purpose::Unlock,
            "jCy3/2JoHClgcwP3xBRHbyQNwGOsloT8uKvgu6fR7H0=",
            "55kYSfTNqLYnhU6jg+WfuZaUkThoZehudbJPPuP/JH0=",
            "OcN4e3gJUGVCUF8chuwGQ0FnhU9vRJ0HowN3zReozxc=",
        ),
    ];

    for (purpose, claim_mac, granted_mac, denied_mac) in expected {
        assert_eq!(
            claim_mac_message_for_purpose(vector.machine, vector.nonce, purpose),
            format!("claim\nkomp\n{}\n{}", vector.nonce, purpose.as_str())
        );
        assert_eq!(
            grant_mac_message_for_purpose(
                vector.machine,
                vector.nonce,
                true,
                vector.grant_id,
                purpose
            ),
            format!(
                "grant\nkomp\n{}\n1\n{}\n{}",
                vector.nonce,
                vector.grant_id,
                purpose.as_str()
            )
        );
        assert_eq!(
            grant_mac_message_for_purpose(vector.machine, vector.nonce, false, "", purpose),
            format!("grant\nkomp\n{}\n0\n\n{}", vector.nonce, purpose.as_str())
        );

        assert_eq!(
            hmac_sha256_b64(
                &vector.key,
                &claim_mac_message_for_purpose(vector.machine, vector.nonce, purpose)
            ),
            claim_mac
        );
        assert_eq!(
            hmac_sha256_b64(
                &vector.key,
                &grant_mac_message_for_purpose(
                    vector.machine,
                    vector.nonce,
                    true,
                    vector.grant_id,
                    purpose
                )
            ),
            granted_mac
        );
        assert_eq!(
            hmac_sha256_b64(
                &vector.key,
                &grant_mac_message_for_purpose(vector.machine, vector.nonce, false, "", purpose)
            ),
            denied_mac
        );

        let granted = verify_claim_response_for_purpose(
            &make_claim_body(true, vector.grant_id, granted_mac),
            &vector.key,
            vector.machine,
            vector.nonce,
            purpose,
        );
        assert_eq!(granted.outcome, ClaimOutcome::Granted, "{purpose:?}");
        assert_eq!(granted.grant_id, vector.grant_id);

        let denied = verify_claim_response_for_purpose(
            &make_claim_body(false, "", denied_mac),
            &vector.key,
            vector.machine,
            vector.nonce,
            purpose,
        );
        assert_eq!(denied.outcome, ClaimOutcome::Denied, "{purpose:?}");
    }

    // A response MAC of the other purpose is rejected: a logon grant can never
    // unlock and an unlock grant can never log on.
    let logon_granted = "kmFSwXXu4g4wylTSIf9DSYht1v28pG4jboomTnocI0k=";
    let unlock_granted = "55kYSfTNqLYnhU6jg+WfuZaUkThoZehudbJPPuP/JH0=";
    assert_eq!(
        verify_claim_response_for_purpose(
            &make_claim_body(true, vector.grant_id, logon_granted),
            &vector.key,
            vector.machine,
            vector.nonce,
            Purpose::Unlock,
        )
        .outcome,
        ClaimOutcome::InvalidMac
    );
    assert_eq!(
        verify_claim_response_for_purpose(
            &make_claim_body(true, vector.grant_id, unlock_granted),
            &vector.key,
            vector.machine,
            vector.nonce,
            Purpose::Logon,
        )
        .outcome,
        ClaimOutcome::InvalidMac
    );
}

#[test]
fn purpose_aware_claim_bodies() {
    let claim = claim_request_json_for_purpose(
        "komp",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
        "jCy3/2JoHClgcwP3xBRHbyQNwGOsloT8uKvgu6fR7H0=",
        Purpose::Unlock,
        Some(25),
    );
    assert_eq!(
        claim,
        "{\"machine\":\"komp\",\"nonce\":\"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=\",\
         \"purpose\":\"unlock\",\"mac\":\"jCy3/2JoHClgcwP3xBRHbyQNwGOsloT8uKvgu6fR7H0=\",\
         \"waitSeconds\":25}"
    );

    let logon = claim_request_json_for_purpose(
        "komp",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
        "AMPMcTPW1kuBTUp8sWtF4weIoI/Vtcp0R+cNBK+xdB0=",
        Purpose::Logon,
        None,
    );
    assert!(logon.contains("\"purpose\":\"logon\""));
    assert!(!logon.contains("waitSeconds"));
}

#[test]
fn base64() {
    assert_eq!(b64_encode(b""), "");
    assert_eq!(b64_encode(b"f"), "Zg==");
    assert_eq!(b64_encode(b"fo"), "Zm8=");
    assert_eq!(b64_encode(b"foo"), "Zm9v");
    assert_eq!(b64_encode(b"foob"), "Zm9vYg==");
    assert_eq!(b64_encode(b"fooba"), "Zm9vYmE=");
    assert_eq!(b64_encode(b"foobar"), "Zm9vYmFy");

    assert_eq!(b64_decode("Zm9vYmFy").unwrap(), b"foobar".to_vec());
    assert_eq!(b64_decode(""), Ok(Vec::new()));

    for rejected in [
        "Zg=",      // wrong length
        "Z===",     // too much padding
        "=Zg=",     // padding first
        "Zg==Zg==", // padding inside
        "Zm9v\n",   // whitespace
        "Zm9v!",    // bad character
        "Zg==x",    // trailing data
    ] {
        assert!(b64_decode(rejected).is_err(), "accepted {rejected:?}");
    }
    // Valid base64 that is not canonical: trailing bits are ignored, exactly
    // as the decoder contract requires.
    assert_eq!(b64_decode("ZZ=="), Ok(vec![0x65]));
}

#[test]
fn challenge_parsing() {
    let response = parse_challenge_response(
        "{\"nonce\":\"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=\",\"expiresIn\":60}",
    )
    .expect("valid challenge");
    assert_eq!(
        response.nonce,
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="
    );
    assert_eq!(response.expires_in, 60);

    parse_challenge_response(
        "  {\"expiresIn\":60,\"nonce\":\"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=\"}\n",
    )
    .expect("leading/trailing space is allowed");

    assert!(parse_challenge_response("{}").is_err());

    // Malformed shapes: none may be accepted.
    let nonce = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
    let bad = [
        "".to_owned(),
        "ok".to_owned(),
        "[]".to_owned(),
        format!("{{\"nonce\":\"{nonce}\"}} trailing"),
        format!("{{\"nonce\":\"{nonce}\",}}"),
        "{\"nonce\":null}".to_owned(),
        "{\"nonce\":123}".to_owned(),
        "{\"nonce\":\"short\"}".to_owned(),
        format!("{{\"nonce\":\"{nonce}\",\"nonce\":\"{nonce}\"}}"),
        format!("{{\"nonce\":\"{nonce}\",\"expiresIn\":0}}"),
        format!("{{\"nonce\":\"{nonce}\",\"expiresIn\":1.5}}"),
        format!("{{\"nonce\":\"{nonce}\",\"expiresIn\":\"60\"}}"),
        format!("{{\"nonce\":\"{nonce}\",\"expiresIn\":7200}}"),
    ];
    for body in bad {
        let error = parse_challenge_response(&body).expect_err("must be rejected");
        assert!(!error.detail().is_empty());
    }
}

#[test]
fn claim_parsing_rejections() {
    let vector = TestVector::new();
    let mac = "UAADAs7pUxn/EN42woIH5K0qrH2YB+bd1SYZVKP//FI=";

    let cases = [
        ("".to_owned(), ClaimOutcome::Malformed),
        ("{}".to_owned(), ClaimOutcome::Malformed),
        ("{\"granted\":true}".to_owned(), ClaimOutcome::Malformed),
        (
            "{\"granted\":true,\"grantId\":\"\",\"mac\":\"\"}".to_owned(),
            ClaimOutcome::Malformed,
        ),
        (
            format!("{{\"granted\":\"yes\",\"mac\":\"{mac}\"}}"),
            ClaimOutcome::Malformed,
        ),
        (
            "{\"granted\":true,\"grantId\":\"x\"}".to_owned(),
            ClaimOutcome::InvalidMac,
        ),
        (
            "{\"granted\":true,\"grantId\":\"x\",\"mac\":null}".to_owned(),
            ClaimOutcome::Malformed,
        ),
        (
            "{\"granted\":true,\"grantId\":\"x\",\"mac\":\"bm90LWEtbWFj\"}".to_owned(),
            ClaimOutcome::InvalidMac,
        ),
        (
            format!("{{\"granted\":false,\"grantId\":\"nonempty\",\"mac\":\"{mac}\"}}"),
            ClaimOutcome::Malformed,
        ),
        (
            format!("{{\"granted\":true,\"grantId\":\"x\",\"mac\":\"{mac}\""),
            ClaimOutcome::Malformed,
        ),
        (
            format!("{{\"granted\":true,\"grantId\":\"x\",\"mac\":\"{mac}\"}}{{}}"),
            ClaimOutcome::Malformed,
        ),
    ];
    for (body, expected) in cases {
        let result = verify_claim_response(&body, &vector.key, vector.machine, vector.nonce);
        assert_eq!(result.outcome, expected, "body: {body}");
        assert!(!result.detail.is_empty());
    }
}

#[test]
fn request_bodies() {
    assert_eq!(challenge_request_json("komp"), "{\"machine\":\"komp\"}");
    let claim = claim_request_json("komp", "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=");
    assert!(claim.contains("\"machine\":\"komp\""));
    assert!(claim.contains("\"nonce\":\"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=\""));
    assert!(claim.contains("\"mac\":\"\""));
}

#[test]
fn secret_file() {
    let key = b64_encode(&byte_range(0x00, 0x1f));
    let secret = parse_secret_json(&format!(
        "{{\"version\":1,\"password\":\"p@ss\\\"word\",\"key\":\"{key}\"}}"
    ))
    .expect("valid secret");
    assert_eq!(secret.password(), "p@ss\"word");
    assert_eq!(secret.key(), byte_range(0x00, 0x1f).as_slice());

    // version is optional and ignored when absent.
    parse_secret_json(&format!("{{\"password\":\"x\",\"key\":\"{key}\"}}")).expect("valid secret");

    let bad = [
        "".to_owned(),
        "{}".to_owned(),
        "{\"password\":\"x\"}".to_owned(),
        format!("{{\"key\":\"{key}\"}}"),
        "{\"password\":\"x\",\"key\":\"c2hvcnQ=\"}".to_owned(),
        format!("{{\"password\":\"\",\"key\":\"{key}\"}}"),
        "{\"password\":\"x\",\"key\":\"\"}".to_owned(),
        format!("{{\"version\":2,\"password\":\"x\",\"key\":\"{key}\"}}"),
        format!("{{\"password\":\"x\",\"key\":\"{key}\",\"extra\":1}}"),
        format!("{{\"password\":\"x\",\"key\":\"{key}\""),
    ];
    for body in bad {
        assert!(
            parse_secret_json(&body).is_err(),
            "accepted secret body {body:?}"
        );
    }
}

/// Server responses are tolerant readers: a new field in wakegrant must not
/// lock the machine out, and the MAC still decides.
#[test]
fn response_unknown_fields_are_ignored() {
    let vector = TestVector::new();

    let challenge = parse_challenge_response(&format!(
        "{{\"nonce\":\"{}\",\"expiresIn\":60,\"extra\":1,\"serverVersion\":\"x\"}}",
        vector.nonce
    ))
    .expect("challenge with unknown fields");
    assert_eq!(challenge.nonce, vector.nonce);
    assert_eq!(challenge.expires_in, 60);

    let granted_mac = "UAADAs7pUxn/EN42woIH5K0qrH2YB+bd1SYZVKP//FI=";
    let claim = verify_claim_response(
        &format!(
            "{{\"granted\":true,\"grantId\":\"{}\",\"mac\":\"{granted_mac}\",\"extra\":1,\"serverVersion\":\"x\"}}",
            vector.grant_id
        ),
        &vector.key,
        vector.machine,
        vector.nonce,
    );
    assert_eq!(claim.outcome, ClaimOutcome::Granted);
    assert_eq!(claim.grant_id, vector.grant_id);
}

#[test]
fn constant_time_equal_checks() {
    assert!(constant_time_equal("abc", "abc"));
    assert!(!constant_time_equal("abc", "abd"));
    assert!(!constant_time_equal("abc", "abcd"));
    assert!(constant_time_equal([1u8, 2, 3], [1u8, 2, 3]));
    assert!(!constant_time_equal([1u8, 2, 3], [1u8, 2, 4]));
}

#[test]
fn secret_debug_is_redacted() {
    let secret = parse_secret_json(&format!(
        "{{\"password\":\"hunter2\",\"key\":\"{}\"}}",
        b64_encode(&byte_range(0, 31))
    ))
    .expect("valid secret");
    let text = format!("{secret:?}");
    assert!(!text.contains("hunter2"));
    assert!(text.contains("<redacted>"));
}
