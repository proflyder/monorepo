use std::sync::atomic::{AtomicU32, Ordering};

use tokio_util::sync::CancellationToken;

use super::*;
use crate::release::Digest;
use crate::test_source::{Fake, FakeAsset, layer, layer_with_digest, release, sha_hex};

/// Временный каталог, удаляется при drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "pulsar-update-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        Self(dir)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.0)
            .expect("read dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// «Бинарь» релиза: отвечает `--version` как pulsar.
fn script(version: &str) -> Vec<u8> {
    format!("#!/bin/sh\necho 'pulsar {version}'\n").into_bytes()
}

/// То же с 300 кБ комментариев после exit: приходит многими чтениями.
fn big_script(version: &str) -> Vec<u8> {
    let mut script = format!("#!/bin/sh\necho 'pulsar {version}'\nexit 0\n").into_bytes();
    for _ in 0..3000 {
        script.extend_from_slice(format!("# {}\n", "x".repeat(97)).as_bytes());
    }
    script
}

const INSTALLER: Installer = Installer::new("host");
const ASSET: &str = "pulsar-host.bin";

/// Установка с пустым прогрессом; ошибка и значения прогресса.
async fn install(
    source: &Fake,
    release: &Release,
    binary: &Path,
    extras: &[Extra],
) -> (Result<(), UpdateError>, Vec<(usize, usize)>) {
    let mut progress = Vec::new();
    let mut record = |got: usize, total: usize| progress.push((got, total));
    let result = INSTALLER
        .install(
            source,
            release,
            binary,
            extras,
            &mut record,
            &CancellationToken::new(),
        )
        .await;
    (result, progress)
}

/// Тесты установки запускают только что записанные файлы: fork другого теста, пока файл
/// ещё открыт на запись, дал бы ETXTBSY.
static EXEC: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[test]
fn tags_and_sums() {
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    let sums = format!(
        "{a}  pulsar-armhf.bin\r\n{b} *pulsar-aarch64.bin\n{}  short.bin\n{a}  zigbee router.ybl\n{b}  last.bin",
        &a[..63]
    );
    let sums = sums.as_bytes();
    assert_eq!(expected_sha(sums, "pulsar-armhf.bin"), Some(a.as_str()));
    assert_eq!(expected_sha(sums, "pulsar-aarch64.bin"), Some(b.as_str()));
    assert_eq!(expected_sha(sums, "zigbee router.ybl"), Some(a.as_str()));
    assert_eq!(expected_sha(sums, "last.bin"), Some(b.as_str()));
    assert_eq!(expected_sha(sums, "short.bin"), None);
    assert_eq!(expected_sha(sums, "pulsar-host.bin"), None);
    assert_eq!(expected_sha(sums, "*pulsar-aarch64.bin"), None);
    assert_eq!(expected_sha(format!("{a} ").as_bytes(), ""), None);
    assert_eq!(expected_sha(b"", "x"), None);
    assert_eq!(
        binary_dir(Path::new("/data/pulsar/pulsar")),
        Path::new("/data/pulsar")
    );
    assert_eq!(binary_dir(Path::new("pulsar")), Path::new("."));
    assert_eq!(binary_dir(Path::new("/pulsar")), Path::new("/"));
}

#[tokio::test]
async fn install_swaps_binary_and_extras() {
    let _exec = EXEC.lock().await;
    let dir = TempDir::new();
    let binary = dir.join("pulsar");
    fs::write(&binary, script("v1.0.0")).expect("binary");
    let new = big_script("v9.9.9");
    let router = b"router image".to_vec();
    let sums = format!(
        "{}  {ASSET}\n{}  zigbee-router.ybl\n",
        sha_hex(&new),
        sha_hex(&router)
    );
    let mut source = Fake::new([
        (ASSET, FakeAsset::Body(new.clone())),
        ("SHA256SUMS", FakeAsset::Body(sums.clone().into_bytes())),
        ("zigbee-router.ybl", FakeAsset::Body(router.clone())),
    ]);
    let release = release(
        "v9.9.9",
        vec![
            layer(ASSET, "/bin", &new),
            layer("SHA256SUMS", "/sums", sums.as_bytes()),
            layer("zigbee-router.ybl", "/router", &router),
        ],
    );
    source = source.with_latest(release.clone());
    let extras = [
        Extra {
            name: "zigbee-router.ybl".into(),
            path: dir.join("zigbee-router.ybl"),
        },
        // Не в этом релизе (404): ставится без него.
        Extra {
            name: "zigbee-coordinator.ybl".into(),
            path: dir.join("zigbee-coordinator.ybl"),
        },
    ];
    let (result, progress) = install(&source, &release, &binary, &extras).await;
    assert_eq!(result, Ok(()));
    assert_eq!(fs::read(&binary).expect("binary"), new);
    assert_eq!(
        fs::read(dir.join("pulsar.prev")).expect(".prev"),
        script("v1.0.0")
    );
    assert_eq!(
        fs::read(dir.join("zigbee-router.ybl")).expect("extra"),
        router
    );
    assert_eq!(progress.last().map(|p| p.1), Some(new.len()));
    let mode = |name: &str| {
        fs::metadata(dir.join(name))
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(
        (
            mode("pulsar"),
            mode("pulsar.prev"),
            mode("zigbee-router.ybl")
        ),
        (0o755, 0o755, 0o644)
    );
    assert_eq!(dir.names(), ["pulsar", "pulsar.prev", "zigbee-router.ybl"]);
}

#[tokio::test]
async fn install_refuses_bad_releases() {
    let _exec = EXEC.lock().await;
    let dir = TempDir::new();
    let binary = dir.join("pulsar");
    let old = script("v1.0.0");
    fs::write(&binary, old.clone()).expect("binary");
    let good = script("v9.9.9");
    let router = b"router".to_vec();
    let good_sums = format!(
        "{}  {ASSET}\n{}  zigbee-router.ybl\n",
        sha_hex(&good),
        sha_hex(&router)
    );
    let digest = |asset: &Option<FakeAsset>, fallback: &[u8]| match asset {
        Some(FakeAsset::Body(body)) => Digest::of(body),
        _ => Digest::of(fallback),
    };
    let attempt = |bin: Option<FakeAsset>, sums: Option<FakeAsset>, extra: Option<FakeAsset>| {
        let bin_digest = digest(&bin, &good);
        let sums_digest = digest(&sums, good_sums.as_bytes());
        let extra_digest = digest(&extra, &router);
        let mut source = Fake::new([]);
        if let Some(bin) = bin {
            source.assets.insert(ASSET.into(), bin);
        }
        if let Some(sums) = sums {
            source.assets.insert("SHA256SUMS".into(), sums);
        }
        if let Some(extra) = extra {
            source.assets.insert("zigbee-router.ybl".into(), extra);
        }
        let release = release(
            "v9.9.9",
            vec![
                layer_with_digest(ASSET, "/bin", bin_digest),
                layer_with_digest("SHA256SUMS", "/sums", sums_digest),
                layer_with_digest("zigbee-router.ybl", "/router", extra_digest),
            ],
        );
        let extras = [Extra {
            name: "zigbee-router.ybl".into(),
            path: dir.join("zigbee-router.ybl"),
        }];
        (source, release, extras)
    };
    let run = async |source: Fake, release: Release, extras: &[Extra]| {
        let (result, _) = install(&source, &release, &binary, extras).await;
        // Ничего не изменилось и не осталось.
        assert_eq!(fs::read(&binary).expect("binary"), old);
        assert_eq!(dir.names(), ["pulsar"]);
        result
    };
    let body = |data: &[u8]| Some(FakeAsset::Body(data.to_vec()));
    let fatal = |e: &str| Err(UpdateError::Fatal(e.to_owned()));

    let (s, r, e) = attempt(None, body(good_sums.as_bytes()), None);
    assert_eq!(run(s, r, &e).await, fatal(&format!("{ASSET}: HTTP 404")));
    let (s, r, e) = attempt(body(&good), None, None);
    assert_eq!(run(s, r, &e).await, fatal("SHA256SUMS: HTTP 404"));
    let moved = format!("{}  pulsar-armhf.bin\n", sha_hex(&good));
    let (s, r, e) = attempt(body(&good), body(moved.as_bytes()), None);
    assert_eq!(
        run(s, r, &e).await,
        fatal(&format!("SHA256SUMS lists no {ASSET}"))
    );
    let wrong_sums = format!("{}  {ASSET}\n", sha_hex(b"other"));
    let (s, r, e) = attempt(body(&good), body(wrong_sums.as_bytes()), None);
    assert_eq!(
        run(s, r, &e).await,
        fatal(&format!("{ASSET}: checksum mismatch"))
    );
    let wrong = script("v1.0.0");
    let wrong_sums = format!("{}  {ASSET}\n", sha_hex(&wrong));
    let (s, r, e) = attempt(body(&wrong), body(wrong_sums.as_bytes()), None);
    assert_eq!(
        run(s, r, &e).await,
        fatal(&format!(
            "{ASSET} does not run here as v9.9.9 (reports 'pulsar v1.0.0')"
        ))
    );
    let silent = b"#!/bin/sh\nexit 3\n".to_vec();
    let silent_sums = format!("{}  {ASSET}\n", sha_hex(&silent));
    let (s, r, e) = attempt(body(&silent), body(silent_sums.as_bytes()), None);
    assert_eq!(
        run(s, r, &e).await,
        fatal(&format!("{ASSET} does not run here as v9.9.9"))
    );
    let (s, r, e) = attempt(
        body(&good),
        body(good_sums.as_bytes()),
        Some(FakeAsset::Body(b"tampered".to_vec())),
    );
    assert_eq!(
        run(s, r, &e).await,
        fatal("zigbee-router.ybl: checksum mismatch")
    );
    let only_bin = format!("{}  {ASSET}\n", sha_hex(&good));
    let (s, r, e) = attempt(
        body(&good),
        body(only_bin.as_bytes()),
        Some(FakeAsset::Body(router.clone())),
    );
    assert_eq!(
        run(s, r, &e).await,
        fatal("SHA256SUMS lists no zigbee-router.ybl")
    );
    // Не вина релиза: стоит повторить.
    let (s, r, e) = attempt(
        body(&good),
        body(good_sums.as_bytes()),
        Some(FakeAsset::Status(503)),
    );
    assert_eq!(
        run(s, r, &e).await,
        Err(UpdateError::Retry("zigbee-router.ybl: HTTP 503".into()))
    );
}

#[tokio::test]
async fn install_refuses_a_wrong_layer_digest() {
    let _exec = EXEC.lock().await;
    let dir = TempDir::new();
    let binary = dir.join("pulsar");
    fs::write(&binary, script("v1.0.0")).expect("binary");
    let new = script("v9.9.9");
    let sums = format!("{}  {ASSET}\n", sha_hex(&new));
    let source = Fake::new([
        (ASSET, FakeAsset::Body(new.clone())),
        ("SHA256SUMS", FakeAsset::Body(sums.clone().into_bytes())),
    ]);
    let release = release(
        "v9.9.9",
        vec![
            layer_with_digest(ASSET, "/bin", Digest::of(b"other")),
            layer("SHA256SUMS", "/sums", sums.as_bytes()),
        ],
    );
    let (result, _) = install(&source, &release, &binary, &[]).await;
    assert_eq!(
        result,
        Err(UpdateError::Fatal(format!("{ASSET}: blob digest mismatch")))
    );
    assert_eq!(dir.names(), ["pulsar"]);
}

#[tokio::test]
async fn install_reports_file_errors() {
    let _exec = EXEC.lock().await;
    let dir = TempDir::new();
    let good = script("v9.9.9");
    let sums = format!("{}  {ASSET}\n", sha_hex(&good));
    let source = Fake::new([
        (ASSET, FakeAsset::Body(good.clone())),
        ("SHA256SUMS", FakeAsset::Body(sums.clone().into_bytes())),
    ]);
    let release = release(
        "v9.9.9",
        vec![
            layer(ASSET, "/bin", &good),
            layer("SHA256SUMS", "/sums", sums.as_bytes()),
        ],
    );
    let missing = dir.join("nodir").join("pulsar");
    let (result, _) = install(&source, &release, &missing, &[]).await;
    let staged = dir.join("nodir").join("pulsar.new");
    assert_eq!(
        result,
        Err(UpdateError::Retry(format!(
            "cannot write {}: No such file or directory",
            staged.display()
        )))
    );
    // Запущенного бинаря нет: .prev не появляется, ничего не подменено.
    let binary = dir.join("pulsar");
    let (result, _) = install(&source, &release, &binary, &[]).await;
    assert_eq!(
        result,
        Err(UpdateError::Retry(
            "cannot keep the current binary as .prev: No such file or directory".into()
        ))
    );
    assert_eq!(dir.names(), Vec::<String>::new());
}
