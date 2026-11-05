use tokio_util::sync::CancellationToken;

use super::*;
use crate::test_server::{self, End, Reply, Server, ok, raw};
use crate::test_source::sha256_digest;

#[test]
fn token_parsing() {
    assert_eq!(
        parse_token(br#"{"token":"abc.DEF-_="}"#),
        Ok("abc.DEF-_=".into())
    );
    assert_eq!(
        parse_token(br#"{"token":"","expires_in":300}"#),
        Err("ghcr.io token: unexpected token".into())
    );
    assert_eq!(
        parse_token(br#"{"token":"a\nb"}"#),
        Err("ghcr.io token: unexpected token".into())
    );
    assert_eq!(
        parse_token(b"{}"),
        Err("ghcr.io token: response has no token".into())
    );
    assert_eq!(
        parse_token(b"token"),
        Err("ghcr.io token: response is not JSON".into())
    );
    // Новые поля документа не мешают.
    assert_eq!(
        parse_token(br#"{"token":"tok","expires_in":300,"issued_at":"now"}"#),
        Ok("tok".into())
    );
}

#[test]
fn repositories_are_strict() {
    assert!(safe_repo("owner/pkg"));
    assert!(safe_repo("a-b_c.d/e.f"));
    assert!(!safe_repo("owner"));
    assert!(!safe_repo("owner/"));
    assert!(!safe_repo("/pkg"));
    assert!(!safe_repo("a/b/c"));
    assert!(!safe_repo("Owner/pkg"));
    assert!(!safe_repo("owner/pkg?x"));

    // Реестр из константы демона снимается, остальное — owner/name.
    let source = Oci::new(&OciConfig {
        repo: "ghcr.io/owner/pkg".into(),
        arch: "host",
        user_agent: "pulsar/test".into(),
        ca_bundle: test_server::ca_file(),
        origin: "https://ghcr.io/".into(),
    })
    .expect("source");
    assert_eq!(source.repo, "owner/pkg");
}

fn json_manifest(version: Option<&str>, layers: &[(&str, &str)]) -> Vec<u8> {
    let layers: Vec<String> = layers
        .iter()
        .map(|(title, digest)| {
            format!(
                r#"{{"digest":"{digest}","annotations":{{"org.opencontainers.image.title":"{title}"}}}}"#
            )
        })
        .collect();
    let annotations = version.map_or_else(String::new, |version| {
        format!(r#","annotations":{{"org.opencontainers.image.version":"{version}"}}"#)
    });
    format!(
        r#"{{"schemaVersion":2,"layers":[{}]{annotations}}}"#,
        layers.join(",")
    )
    .into_bytes()
}

#[test]
fn oci_manifest_parsing() {
    let bin = b"bin";
    let sums = b"sums";
    let router = b"router";
    let asset = "pulsar-host.bin";
    let layers = [
        (asset, sha256_digest(bin)),
        ("SHA256SUMS", sha256_digest(sums)),
        ("zigbee-router.ybl", sha256_digest(router)),
    ];
    let refs: Vec<(&str, &str)> = layers.iter().map(|(t, d)| (*t, d.as_str())).collect();
    let manifest = json_manifest(Some("v1.2.3"), &refs);
    let release =
        parse_manifest(&manifest, "owner/pkg", "https://ghcr.io/", "host").expect("release");
    assert_eq!(release.version().as_str(), "v1.2.3");
    let bin_url = format!("https://ghcr.io/v2/owner/pkg/blobs/{}", sha256_digest(bin));
    let sums_url = format!("https://ghcr.io/v2/owner/pkg/blobs/{}", sha256_digest(sums));
    assert_eq!(release.asset_url(asset), Some(bin_url.as_str()));
    assert_eq!(release.asset_url("SHA256SUMS"), Some(sums_url.as_str()));
    assert!(release.layer("zigbee-router.ybl").is_some());
    assert_eq!(
        release.page_url(),
        "https://github.com/users/owner/packages/container/package/pkg"
    );

    let bad =
        |body: &[u8]| parse_manifest(body, "owner/pkg", "https://ghcr.io/", "host").unwrap_err();
    assert_eq!(
        bad(&json_manifest(None, &refs)),
        "manifest has no org.opencontainers.image.version"
    );
    for version in ["1.2.3", "v1.2", "v1.2.3-rc1", "latest", "v1.2.x"] {
        assert_eq!(
            bad(&json_manifest(Some(version), &refs)),
            format!("unexpected image version {version}")
        );
    }
    assert_eq!(
        bad(&json_manifest(Some("v1.2.3"), &refs[..1])),
        "manifest has no SHA256SUMS layer"
    );
    let other_arch = [("pulsar-armhf.bin", refs[0].1), ("SHA256SUMS", refs[1].1)];
    let other_arch: Vec<(&str, &str)> = other_arch.iter().map(|(t, d)| (*t, *d)).collect();
    assert_eq!(
        bad(&json_manifest(Some("v1.2.3"), &other_arch)),
        "manifest has no pulsar-host.bin layer"
    );
    let bad_digest = [(asset, "sha256:12"), ("SHA256SUMS", refs[1].1)];
    assert_eq!(
        bad(&json_manifest(Some("v1.2.3"), &bad_digest)),
        "layer pulsar-host.bin: unexpected digest sha256:12"
    );
    assert_eq!(bad(b"not json"), "manifest is not JSON");
    assert_eq!(
        bad(br#"{"annotations":{"org.opencontainers.image.version":"v1.2.3"}}"#),
        "manifest has no layers"
    );
}

fn source(server: &Server, repo: &str) -> Oci {
    Oci::new(&OciConfig {
        repo: repo.into(),
        arch: "host",
        user_agent: "pulsar/v1.0.0".into(),
        ca_bundle: test_server::ca_file(),
        origin: server.url("/"),
    })
    .expect("source")
}

#[tokio::test]
async fn oci_check_reads_the_token_and_manifest() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let server = Server::start().await;
            server.route(
                "/token?scope=repository:owner/repo:pull&service=ghcr.io",
                ok(br#"{"token":"tok123"}"#),
            );
            let asset = "pulsar-host.bin";
            let layers = [
                (asset, sha256_digest(b"bin")),
                ("SHA256SUMS", sha256_digest(b"sums")),
            ];
            let refs: Vec<(&str, &str)> = layers.iter().map(|(t, d)| (*t, d.as_str())).collect();
            server.route(
                "/v2/owner/repo/manifests/latest",
                ok(&json_manifest(Some("v1.2.3"), &refs)),
            );
            let release = source(&server, "owner/repo")
                .latest(&CancellationToken::new())
                .await
                .expect("release");
            assert_eq!(release.version().as_str(), "v1.2.3");
            let heads = server.requests();
            assert!(heads[0].starts_with("GET /token?scope="), "{}", heads[0]);
            assert!(
                !heads[0].to_ascii_lowercase().contains("authorization:"),
                "{}",
                heads[0]
            );
            assert!(
                heads[1].starts_with("GET /v2/owner/repo/manifests/latest HTTP/1.1\r\n"),
                "{}",
                heads[1]
            );
            assert!(
                heads[1]
                    .to_ascii_lowercase()
                    .contains("\r\nauthorization: bearer tok123\r\n"),
                "{}",
                heads[1]
            );
            assert!(
                heads[1]
                    .to_ascii_lowercase()
                    .contains("\r\naccept: application/vnd.oci.image.manifest.v1+json\r\n"),
                "{}",
                heads[1]
            );
            assert!(
                heads.iter().all(|head| head
                    .to_ascii_lowercase()
                    .contains("\r\nuser-agent: pulsar/v1.0.0\r\n")),
                "{heads:?}"
            );
        })
        .await;
}

#[tokio::test]
async fn oci_reports_statuses_and_transport_errors() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let server = Server::start().await;
            let cancel = CancellationToken::new();
            server.route(
                "/token?scope=repository:owner/repo:pull&service=ghcr.io",
                raw("HTTP/1.1 500 Busy\r\nContent-Length: 0\r\n\r\n"),
            );
            assert_eq!(
                source(&server, "owner/repo").latest(&cancel).await,
                Err(UpdateError::Retry("ghcr.io token: HTTP 500".into()))
            );

            server.route(
                "/token?scope=repository:owner/repo:pull&service=ghcr.io",
                ok(br#"{"token":"tok"}"#),
            );
            server.route(
                "/v2/owner/repo/manifests/latest",
                raw("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n"),
            );
            assert_eq!(
                source(&server, "owner/repo").latest(&cancel).await,
                Err(UpdateError::Retry("ghcr.io: HTTP 404".into()))
            );

            // Закрытое соединение до ответа: ошибка транспорта, не паника.
            server.route(
                "/v2/owner/repo/manifests/latest",
                Reply {
                    bytes: b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nshort".to_vec(),
                    end: End::Drop,
                },
            );
            let result = source(&server, "owner/repo").latest(&cancel).await;
            assert!(matches!(result, Err(UpdateError::Retry(_))), "{result:?}");
        })
        .await;
}
