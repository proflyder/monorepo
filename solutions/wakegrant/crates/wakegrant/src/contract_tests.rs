//! Replays the golden HTTP fixtures from `solutions/wakegrant/testdata/contract`
//! through the Rust service. The fixtures are the Go implementation's files,
//! unchanged.

use std::sync::Arc;

use axum::Router;
use home_contracttest::{Options as ContractOptions, run};
use home_secret::Token;
use time::OffsetDateTime;
use time::macros::datetime;
use wakegrant_protocol::b64_decode;

use crate::api::{Options, Service};
use crate::config::Keys;
use crate::domain::{Clock, NONCE_TTL, Purpose};
use crate::store::Store;

const NOW: OffsetDateTime = datetime!(2026-11-03 12:00:00 UTC);
const TOKEN: &str = "contract-issuer-token";
const VECTOR_GRANT_ID: &str = "00000000-0000-4000-8000-000000000001";
const VECTOR_NONCE: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
const SECOND_NONCE: &str = "ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8=";

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        NOW
    }
}

fn vector_key() -> Vec<u8> {
    (0u8..=0x1f).collect()
}

fn keys() -> Keys {
    let mut keys = Keys::new();
    keys.insert("komp".to_owned(), vector_key());
    keys.insert("desk".to_owned(), vec![7u8; 32]);
    keys
}

fn new_router(scenario: &str) -> Router {
    let store = Arc::new(Store::open_in_memory().expect("contract store"));
    let service = Arc::new(Service::new(Options {
        keys: keys(),
        issuer_token: Token::new(TOKEN),
        store: Arc::clone(&store),
        max_ttl: crate::domain::MAX_TTL,
        clock: Arc::new(FixedClock),
    }));
    service.nonces().insert_for_test("komp", VECTOR_NONCE, NOW);
    service.nonces().insert_for_test("komp", SECOND_NONCE, NOW);
    if scenario != "claim_denied_vector" {
        store
            .insert_grant_for_test(VECTOR_GRANT_ID, "komp", Purpose::Logon, NOW, NONCE_TTL)
            .expect("seed vector grant");
    }
    service.router()
}

#[tokio::test(flavor = "multi_thread")]
async fn contract_fixtures() {
    // The key vector and nonce are exercised through the fixtures themselves;
    // this assertion only proves the seeded bytes are the documented ones.
    assert_eq!(b64_decode(VECTOR_NONCE).expect("vector nonce").len(), 32);
    // Bazel runs with TEST_SRCDIR/runfiles; cargo runs from the package dir
    // and exports CARGO_MANIFEST_DIR at compile time.
    let dir = if std::env::var_os("TEST_SRCDIR").is_some() {
        "solutions/wakegrant/testdata/contract".to_owned()
    } else {
        option_env!("CARGO_MANIFEST_DIR").map_or_else(
            || "solutions/wakegrant/testdata/contract".to_owned(),
            |root| format!("{root}/../../testdata/contract"),
        )
    };
    let count = run(dir, new_router, ContractOptions::default())
        .await
        .expect("all wakegrant contract fixtures pass");
    assert_eq!(count, 4);
}
