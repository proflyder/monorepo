//! Port of `solutions/windows-logon-provider/tests/test_protocol.cpp`.
//!
//! Every assertion from the C++ suite is kept, including the golden HMAC
//! vectors pinned by `packages/contracts/wakegrant/PROTOCOL.md`.

use logon_protocol::{
    ClaimOutcome, b64_decode, b64_encode, challenge_request_json, claim_mac_message,
    claim_request_json, constant_time_equal, endpoint_path, grant_mac_message, hmac_sha256_b64,
    parse_challenge_response, parse_secret_json, parse_service_url, redact_service_url,
    validate_config, verify_claim_response,
};

fn byte_range(first: u8, last: u8) -> Vec<u8> {
    (first..=last).collect()
}

fn make_claim_body(granted: bool, grant_id: &str, mac: &str) -> String {
    let mut body = format!("{{\"granted\":{granted}");
    if !grant_id.is_empty() {
        body.push_str(&format!(",\"grantId\":\"{grant_id}\""));
    }
    body.push_str(&format!(",\"mac\":\"{mac}\"}}"));
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

    // Valid granted response.
    let granted_mac = "UAADAs7pUxn/EN42woIH5K0qrH2YB+bd1SYZVKP//FI=";
    let granted = verify_claim_response(
        &make_claim_body(true, vector.grant_id, granted_mac),
        &vector.key,
        vector.machine,
        vector.nonce,
    );
    assert_eq!(granted.outcome, ClaimOutcome::Granted);
    assert_eq!(granted.grant_id, vector.grant_id);

    // Valid denied response.
    let denied = verify_claim_response(
        &make_claim_body(false, "", "I6ClmYKmmfO6wodeKXOobAtJj/SgQ7C/ieNumDYrj1Y="),
        &vector.key,
        vector.machine,
        vector.nonce,
    );
    assert_eq!(denied.outcome, ClaimOutcome::Denied);
    assert!(denied.grant_id.is_empty());

    // A granted body cannot carry the denied mac.
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

    // A denied body cannot carry the granted mac.
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
    // A different grant id invalidates the mac as well.
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
    // like the C++ decoder.
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

    // Empty nonce field and a non-object body.
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
        format!("{{\"nonce\":\"{nonce}\",\"extra\":{{\"a\":1}}}}"),
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
fn config_validation() {
    let normalized =
        validate_config("http://192.0.2.27:30130", "komp", "pl", ".").expect("valid configuration");
    assert_eq!(normalized, "http://192.0.2.27:30130");
    assert_eq!(
        validate_config("http://192.0.2.27:30130/", "komp", "pl", ".").unwrap(),
        "http://192.0.2.27:30130"
    );
    assert_eq!(
        validate_config("https://home.example:8443/wake/", "komp-2", "pl", ".").unwrap(),
        "https://home.example:8443/wake"
    );
    assert_eq!(
        validate_config("http://[fd00::1]:30130", "komp", "pl", ".").unwrap(),
        "http://[fd00::1]:30130"
    );
    assert_eq!(
        validate_config("http://example.lan", "komp", "pl", ".").unwrap(),
        "http://example.lan"
    );

    let invalid = [
        ("", "komp", "pl", "."),
        ("ftp://192.0.2.27", "komp", "pl", "."),
        ("192.0.2.27:30130", "komp", "pl", "."),
        ("http://", "komp", "pl", "."),
        ("http://192.0.2.27:0", "komp", "pl", "."),
        ("http://192.0.2.27:70000", "komp", "pl", "."),
        ("http://192.0.2.27:abc", "komp", "pl", "."),
        ("http://user:pass@192.0.2.27", "komp", "pl", "."),
        ("http://192.0.2.27/?x=1", "komp", "pl", "."),
        ("http://192.0.2.27/#frag", "komp", "pl", "."),
        ("http://192.0.2.27/a//b", "komp", "pl", "."),
        ("http://192.0.2.27 /x", "komp", "pl", "."),
        ("http://192.0.2.27", "", "pl", "."),
        ("http://192.0.2.27", "Komp", "pl", "."),
        ("http://192.0.2.27", "komp_1", "pl", "."),
        ("http://192.0.2.27", "komp", "", "."),
        ("http://192.0.2.27", "komp", "p l", "."),
        ("http://192.0.2.27", "komp", "a\\b", "."),
        ("http://192.0.2.27", "komp", "pl", ""),
        ("http://192.0.2.27", "komp", "pl", "a b"),
        (
            "http://192.0.2.27",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "pl",
            ".",
        ),
    ];
    for (url, machine, user, domain) in invalid {
        let error = validate_config(url, machine, user, domain)
            .expect_err(&format!("accepted {url} {machine} {user} {domain:?}"));
        assert!(!error.detail().is_empty());
    }

    let endpoint = parse_service_url("http://192.0.2.27:30130/wake").expect("valid url");
    assert_eq!(endpoint.host, "192.0.2.27");
    assert_eq!(endpoint.port, 30130);
    assert!(!endpoint.tls);
    assert_eq!(endpoint.prefix, "/wake");
    assert_eq!(endpoint_path(&endpoint, "/v1/claim"), "/wake/v1/claim");

    let endpoint = parse_service_url("https://example.lan").expect("valid url");
    assert_eq!(endpoint.port, 443);
    assert!(endpoint.tls);
    assert_eq!(endpoint_path(&endpoint, "/v1/claim"), "/v1/claim");

    let endpoint = parse_service_url("http://[fd00::1]").expect("valid url");
    assert_eq!(endpoint.host, "fd00::1");
    assert_eq!(endpoint.port, 80);

    assert_eq!(
        redact_service_url("http://192.0.2.27:30130/secret/path"),
        "http://192.0.2.27:30130"
    );
    assert_eq!(redact_service_url("not a url"), "invalid-url");
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
        format!("{{\"password\":\"x\",\"key\":\"{key}\""),
    ];
    for body in bad {
        assert!(
            parse_secret_json(&body).is_err(),
            "accepted secret body {body:?}"
        );
    }
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
