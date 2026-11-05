use std::fmt::Write as _;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use std::path::Path;

use super::*;
use crate::test_server::{self, End, Reply, Server, ok, raw, redirect};
use crate::test_source::{layer, release, sha_hex, sha256_digest};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "pulsar-update-actor-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        Self(dir)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.0)
            .expect("read dir")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn script(version: &str) -> Vec<u8> {
    format!("#!/bin/sh\necho 'pulsar {version}'\n").into_bytes()
}

fn big_script(version: &str) -> Vec<u8> {
    let mut script = format!("#!/bin/sh\necho 'pulsar {version}'\nexit 0\n").into_bytes();
    for _ in 0..3000 {
        script.extend_from_slice(format!("# {}\n", "x".repeat(97)).as_bytes());
    }
    script
}

fn spawn_oci(server: &Server, repo: &str) -> (UpdaterHandle, UnboundedReceiver<UpdateEvent>) {
    spawn_source(
        Source::new(&OciConfig {
            repo: repo.into(),
            arch: "host",
            user_agent: "pulsar/v1.0.0".into(),
            ca_bundle: test_server::ca_file(),
            origin: server.url("/"),
        }),
        "host",
    )
}

fn token_route(server: &Server) {
    server.route(
        "/token?scope=repository:owner/repo:pull&service=ghcr.io",
        ok(br#"{"token":"tok"}"#),
    );
}

fn oci_release(
    server: &Server,
    version: &str,
    bin: &[u8],
    sums: &[u8],
    router: Option<&[u8]>,
) -> Release {
    let mut layers = vec![
        layer(
            "pulsar-host.bin",
            &server.url("/v2/owner/repo/blobs/sha256:bin"),
            bin,
        ),
        layer(
            "SHA256SUMS",
            &server.url("/v2/owner/repo/blobs/sha256:sums"),
            sums,
        ),
    ];
    if let Some(router) = router {
        layers.push(layer(
            "zigbee-router.ybl",
            &server.url("/v2/owner/repo/blobs/sha256:router"),
            router,
        ));
    }
    release(version, layers)
}

fn route_assets(server: &Server, bin: &[u8], sums: &[u8], router: Option<&[u8]>) {
    token_route(server);
    server.route("/v2/owner/repo/blobs/sha256:bin", ok(bin));
    server.route("/v2/owner/repo/blobs/sha256:sums", ok(sums));
    if let Some(router) = router {
        server.route("/v2/owner/repo/blobs/sha256:router", ok(router));
    }
}

async fn local<F: Future>(future: F) -> F::Output {
    tokio::task::LocalSet::new().run_until(future).await
}

#[test]
fn arch_is_the_build_target() {
    let expected = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "arm") {
        "armhf"
    } else {
        "host"
    };
    assert_eq!(arch(), expected);
}

#[tokio::test]
async fn check_finds_the_latest_release() {
    local(async {
        let server = Server::start().await;
        let asset = "pulsar-host.bin";
        let layers = [
            (asset, sha256_digest(b"bin")),
            ("SHA256SUMS", sha256_digest(b"sums")),
        ];
        let refs: Vec<(&str, &str)> = layers.iter().map(|(t, d)| (*t, d.as_str())).collect();
        let layer_json = |title: &str, digest: &str| {
            format!(
                r#"{{"digest":"{digest}","annotations":{{"org.opencontainers.image.title":"{title}"}}}}"#
            )
        };
        let bin_layer = layer_json(asset, refs[0].1);
        let sums_layer = layer_json("SHA256SUMS", refs[1].1);
        let manifest = format!(
            r#"{{"schemaVersion":2,"layers":[{bin_layer},{sums_layer}],"annotations":{{"org.opencontainers.image.version":"v1.2.3"}}}}"#
        );
        token_route(&server);
        server.route("/v2/owner/repo/manifests/latest", ok(manifest.as_bytes()));
        let (updater, mut events) = spawn_oci(&server, "owner/repo");
        assert!(!updater.busy());
        assert_eq!(updater.check(), Some(RequestId(1)));
        assert!(updater.busy());
        // Занят: отказ без события.
        assert_eq!(updater.check(), None);
        assert_eq!(
            updater.install(release("v1.0.0", vec![]), PathBuf::from("/nonexistent/pulsar")),
            None
        );
        let Some(UpdateEvent::Checked { id, result }) = events.recv().await else {
            panic!("no Checked");
        };
        assert!(!updater.busy());
        assert_eq!(id, RequestId(1));
        let release = result.expect("release");
        assert_eq!(release.version().as_str(), "v1.2.3");
        // Счётчик продолжается.
        assert_eq!(updater.check(), Some(RequestId(2)));
        let Some(UpdateEvent::Checked { id, result }) = events.recv().await else {
            panic!("no Checked");
        };
        assert_eq!(id, RequestId(2));
        assert!(result.is_ok());
    })
    .await;
}

#[tokio::test]
async fn check_reports_a_failed_source() {
    local(async {
        let server = Server::start().await;
        token_route(&server);
        server.route(
            "/v2/owner/repo/manifests/latest",
            raw("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n"),
        );
        let (updater, mut events) = spawn_oci(&server, "owner/repo");
        updater.check();
        let Some(UpdateEvent::Checked { result, .. }) = events.recv().await else {
            panic!("no Checked");
        };
        assert_eq!(result, Err(UpdateError::Retry("ghcr.io: HTTP 404".into())));
    })
    .await;
}

struct Installed {
    result: Result<(), UpdateError>,
    progress: Vec<f32>,
}

async fn install(
    updater: &UpdaterHandle,
    events: &mut UnboundedReceiver<UpdateEvent>,
    release: Release,
    binary: &Path,
) -> Installed {
    let id = updater
        .install(release, binary.to_path_buf())
        .expect("idle");
    let mut progress = Vec::new();
    loop {
        match events.recv().await {
            Some(UpdateEvent::Progress { id: got, value }) if got == id => progress.push(value),
            Some(UpdateEvent::Installed { id: got, result }) if got == id => {
                assert!(!updater.busy());
                return Installed { result, progress };
            }
            other => panic!("{other:?}"),
        }
    }
}

/// Тесты установки запускают только что записанные файлы: fork параллельного теста, пока
/// файл открыт на запись, дал бы ETXTBSY.
static EXEC: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn install_reads_oci_layers() {
    let _exec = EXEC.lock().await;
    local(async {
        let server = Server::start().await;
        let dir = TempDir::new();
        let binary = dir.join("pulsar");
        std::fs::write(&binary, script("v1.0.0")).expect("binary");
        let new = script("v9.9.9");
        let router = b"router image".to_vec();
        let sums = format!(
            "{}  pulsar-host.bin\n{}  zigbee-router.ybl\n",
            sha_hex(&new),
            sha_hex(&router)
        );
        route_assets(&server, &new, sums.as_bytes(), Some(&router));
        let (updater, mut events) = spawn_oci(&server, "owner/repo");
        updater.add_extra_asset("zigbee-router.ybl", dir.join("zigbee-router.ybl"));
        let done = install(
            &updater,
            &mut events,
            oci_release(&server, "v9.9.9", &new, sums.as_bytes(), Some(&router)),
            &binary,
        )
        .await;
        assert_eq!(done.result, Ok(()));
        assert_eq!(std::fs::read(&binary).expect("binary"), new);
        assert_eq!(
            std::fs::read(dir.join("zigbee-router.ybl")).expect("router"),
            router
        );
        // Загрузки слоёв идут с токеном, запрос токена — без.
        for head in server.requests() {
            if head.starts_with("GET /token?") {
                assert!(
                    !head.to_ascii_lowercase().contains("authorization:"),
                    "{head}"
                );
            } else if head.starts_with("GET /v2/owner/repo/blobs/") {
                assert!(
                    head.to_ascii_lowercase()
                        .contains("\r\nauthorization: bearer tok\r\n"),
                    "{head}"
                );
            }
        }
    })
    .await;
}

#[tokio::test]
async fn install_swaps_binary_and_extras() {
    let _exec = EXEC.lock().await;
    local(async {
        let server = Server::start().await;
        let dir = TempDir::new();
        let binary = dir.join("pulsar");
        std::fs::write(&binary, script("v1.0.0")).expect("binary");
        let new = big_script("v9.9.9");
        let router = b"router image".to_vec();
        server.route("/v2/owner/repo/blobs/sha256:router", ok(&router));
        // Ассеты релиза на GitHub редиректят на другой хост; здесь — на тот же.
        server.route("/v2/owner/repo/blobs/sha256:sums", redirect("/cdn/sums"));
        server.route(
            "/cdn/sums",
            ok(format!(
                "{}  pulsar-host.bin\n{}  zigbee-router.ybl\n",
                sha_hex(&new),
                sha_hex(&router)
            )
            .as_bytes()),
        );
        token_route(&server);
        server.route("/v2/owner/repo/blobs/sha256:bin", ok(&new));
        let sums = format!(
            "{}  pulsar-host.bin\n{}  zigbee-router.ybl\n",
            sha_hex(&new),
            sha_hex(&router)
        );
        let (updater, mut events) = spawn_oci(&server, "owner/repo");
        updater.add_extra_asset("zigbee-router.ybl", dir.join("zigbee-router.ybl"));
        updater.add_extra_asset("zigbee-coordinator.ybl", dir.join("zigbee-coordinator.ybl"));
        let done = install(
            &updater,
            &mut events,
            oci_release(&server, "v9.9.9", &new, sums.as_bytes(), Some(&router)),
            &binary,
        )
        .await;
        assert_eq!(done.result, Ok(()));
        // got * 95 / total, каждое значение один раз.
        assert!(done.progress.len() > 3, "{:?}", done.progress);
        assert_eq!(done.progress.last(), Some(&95.0));
        assert!(done.progress.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(std::fs::read(&binary).expect("binary"), new);
        assert_eq!(
            std::fs::read(dir.join("pulsar.prev")).expect(".prev"),
            script("v1.0.0")
        );
        assert_eq!(dir.names(), ["pulsar", "pulsar.prev", "zigbee-router.ybl"]);
    })
    .await;
}

#[tokio::test]
async fn dropping_the_handles_stops_a_download() {
    local(async {
        let server = Server::start().await;
        token_route(&server);
        server.route(
            "/v2/owner/repo/blobs/sha256:bin",
            Reply {
                bytes: b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\npartial".to_vec(),
                end: End::Hang,
            },
        );
        let (updater, mut events) = spawn_oci(&server, "owner/repo");
        let id = updater.install(
            oci_release(&server, "v9.9.9", b"bin", b"sums", None),
            PathBuf::from("/nonexistent/pulsar"),
        );
        assert_eq!(id, Some(RequestId(1)));
        assert!(matches!(
            events.recv().await,
            Some(UpdateEvent::Progress { .. })
        ));
        drop(updater);
        let start = std::time::Instant::now();
        assert!(
            pulsar_core::supervisor()
                .shutdown(Duration::from_secs(3))
                .await
                .is_empty()
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        // Отменённая установка ничего не сообщает.
        assert!(events.recv().await.is_none());
    })
    .await;
}

/// Запускает установку, ждёт `ready`, закрывает handle и проверяет, что ничего не тронуто.
async fn cancel_after_download(
    bin: Vec<u8>,
    extra: Option<Reply>,
    ready: impl Fn(&Server, &TempDir) -> bool,
) {
    let server = Server::start().await;
    let dir = TempDir::new();
    let binary = dir.join("pulsar");
    let old = script("v1.0.0");
    std::fs::write(&binary, &old).expect("binary");
    route_assets(&server, &bin, b"", None);
    let mut sums = format!("{}  pulsar-host.bin\n", sha_hex(&bin));
    let (updater, mut events) = spawn_oci(&server, "owner/repo");
    if let Some(extra) = extra {
        let _ = writeln!(sums, "{}  zigbee-router.ybl", sha_hex(b"router"));
        server.route("/v2/owner/repo/blobs/sha256:router", extra);
        updater.add_extra_asset("zigbee-router.ybl", dir.join("zigbee-router.ybl"));
    }
    server.route("/v2/owner/repo/blobs/sha256:sums", ok(sums.as_bytes()));
    let release = oci_release(&server, "v9.9.9", &bin, sums.as_bytes(), Some(b"router"));
    assert_eq!(updater.install(release, binary.clone()), Some(RequestId(1)));
    let start = std::time::Instant::now();
    while !ready(&server, &dir) {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{:?}",
            dir.names()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    drop(updater);
    let cancelled = std::time::Instant::now();
    assert!(
        pulsar_core::supervisor()
            .shutdown(Duration::from_secs(3))
            .await
            .is_empty()
    );
    assert!(cancelled.elapsed() < Duration::from_secs(1));
    while let Some(event) = events.recv().await {
        assert!(matches!(event, UpdateEvent::Progress { .. }), "{event:?}");
    }
    assert_eq!(dir.names(), ["pulsar"]);
    assert_eq!(std::fs::read(&binary).expect("binary"), old);
}

#[tokio::test]
async fn cancel_stops_install_during_version_run() {
    let _exec = EXEC.lock().await;
    // Отвечает --version только через 5 с: отмена во время пробы (pulsar.new уже записан).
    let slow = b"#!/bin/sh\nsleep 5\necho 'pulsar v9.9.9'\n".to_vec();
    local(cancel_after_download(slow, None, |_, dir| {
        dir.names().contains(&"pulsar.new".to_owned())
    }))
    .await;
}

#[tokio::test]
async fn cancel_stops_install_before_swap() {
    let _exec = EXEC.lock().await;
    // Бинарь проходит --version; дополнительный ассет не приходит, отмена до подмены.
    let hang = Reply {
        bytes: b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nro".to_vec(),
        end: End::Hang,
    };
    local(cancel_after_download(
        script("v9.9.9"),
        Some(hang),
        |server, _| {
            server
                .requests()
                .iter()
                .any(|head| head.starts_with("GET /v2/owner/repo/blobs/sha256:router "))
        },
    ))
    .await;
}
