//! Identity and pairing storage behind [`IdentityStore`]. The file implementation keeps the C++
//! daemon's exact format (one file per persistence key, raw bytes, the `unpaired_access` switch),
//! so a pairing made by either binary keeps working under the other; the in-memory one is for
//! tests.

#[cfg(test)]
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::{Mutex, PoisonError};

use tracing::{error, warn};

use home_linux::io_error_text as errstr;

/// File whose first non-blank character `0` turns unpaired access off (pairing required).
pub const UNPAIRED_ACCESS: &str = "unpaired_access";

/// Persistence provider the sendspin-cpp client calls from its main loop thread.
pub trait IdentityStore: Send + Sync {
    /// Creates the storage directory if needed; false when identity will not persist.
    fn ensure(&self) -> bool;
    fn load(&self, key: &OsStr) -> Option<Vec<u8>>;
    /// true once durable.
    fn save(&self, key: &OsStr, data: &[u8]) -> bool;
    /// Music Assistant may use the speaker without a pairing record.
    fn unpaired_access(&self) -> bool;
}

/// `<dir>/<key>`, byte for byte as the C++ daemon built it.
fn path(dir: &Path, key: &OsStr) -> PathBuf {
    let mut p = OsString::from(dir.as_os_str());
    p.push("/");
    p.push(key);
    PathBuf::from(p)
}

/// mkdir 0700, not recursive; an existing entry counts as success.
pub fn ensure_dir(dir: &Path) -> bool {
    match DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => true,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => true,
        Err(e) => {
            error!("sendspin: cannot create {}: {}", dir.display(), errstr(&e));
            false
        }
    }
}

pub struct FileStore {
    dir: PathBuf,
}

impl FileStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl IdentityStore for FileStore {
    fn ensure(&self) -> bool {
        ensure_dir(&self.dir)
    }

    fn load(&self, key: &OsStr) -> Option<Vec<u8>> {
        fs::read(path(&self.dir, key)).ok()
    }

    /// Atomic and durable: `<key>.tmp` (0600) written and fsynced, renamed over `<key>`, then the
    /// directory fsynced.
    fn save(&self, key: &OsStr, data: &[u8]) -> bool {
        let p = path(&self.dir, key);
        let mut tmp = OsString::from(p.as_os_str());
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let mut f = match OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
        {
            Ok(f) => f,
            Err(e) => {
                warn!("sendspin: cannot write {}: {}", tmp.display(), errstr(&e));
                return false;
            }
        };
        let written = f.write_all(data).and_then(|()| f.sync_all());
        drop(f);
        if let Err(e) = written.and_then(|()| fs::rename(&tmp, &p)) {
            warn!(
                "sendspin: cannot store {}: {}",
                key.to_string_lossy(),
                errstr(&e)
            );
            let _ = fs::remove_file(&tmp);
            return false;
        }
        if let Ok(d) = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY)
            .open(&self.dir)
        {
            let _ = d.sync_all();
        }
        true
    }

    fn unpaired_access(&self) -> bool {
        fs::read(path(&self.dir, OsStr::new(UNPAIRED_ACCESS))).map_or(true, |b| unpaired_from(&b))
    }
}

/// Pure in-memory keep for tests.
#[cfg(test)]
#[derive(Default)]
pub struct MemoryStore {
    map: Mutex<HashMap<OsString, Vec<u8>>>,
    /// None: the stored switch is absent (on).
    unpaired: Mutex<Option<bool>>,
}

#[cfg(test)]
impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_unpaired_access(self, on: bool) -> Self {
        *self.unpaired.lock().unwrap_or_else(PoisonError::into_inner) = Some(on);
        self
    }
}

#[cfg(test)]
impl IdentityStore for MemoryStore {
    fn ensure(&self) -> bool {
        true
    }

    fn load(&self, key: &OsStr) -> Option<Vec<u8>> {
        self.map
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned()
    }

    fn save(&self, key: &OsStr, data: &[u8]) -> bool {
        self.map
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.to_owned(), data.to_vec());
        true
    }

    fn unpaired_access(&self) -> bool {
        self.unpaired
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .unwrap_or(true)
    }
}

/// C++ `f >> c` (skips C-locale white space), then `c != '0'`; nothing to read = on.
fn unpaired_from(content: &[u8]) -> bool {
    content
        .iter()
        .find(|c| !matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .is_none_or(|&c| c != b'0')
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    struct TmpDir(PathBuf);

    impl TmpDir {
        fn new() -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let p = std::env::temp_dir().join(format!(
                "pulsar-sendspin-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&p);
            Self(p)
        }
    }

    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn mode(p: &Path) -> u32 {
        fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn dir_is_created_0700_once() {
        let t = TmpDir::new();
        assert!(ensure_dir(&t.0));
        assert_eq!(mode(&t.0), 0o700);
        assert!(ensure_dir(&t.0));
        // Not recursive, like mkdir(2).
        assert!(!ensure_dir(&t.0.join("a/b")));
    }

    #[test]
    fn blobs_round_trip_as_raw_files() {
        let t = TmpDir::new();
        let store = FileStore::new(t.0.clone());
        assert!(store.ensure());
        let key = OsStr::new("keypair");
        assert_eq!(store.load(key), None);

        let blob: Vec<u8> = (0..32).collect();
        assert!(store.save(key, &blob));
        let file = t.0.join("keypair");
        assert_eq!(fs::read(&file).unwrap(), blob);
        assert_eq!(mode(&file), 0o600);
        assert!(!t.0.join("keypair.tmp").exists());
        assert_eq!(store.load(key), Some(blob));

        // Overwrite with a shorter blob; an empty one is a valid stored value.
        assert!(store.save(key, &[7, 8]));
        assert_eq!(store.load(key), Some(vec![7, 8]));
        assert!(store.save(OsStr::new("rec_0"), &[]));
        assert_eq!(store.load(OsStr::new("rec_0")), Some(vec![]));

        // A file written by the C++ daemon is read back as is.
        fs::write(t.0.join("output_delay"), [0x2c, 0x01]).unwrap();
        assert_eq!(
            store.load(OsStr::new("output_delay")),
            Some(vec![0x2c, 0x01])
        );
    }

    #[test]
    fn save_fails_without_the_directory() {
        let t = TmpDir::new();
        let store = FileStore::new(t.0.clone());
        assert!(!store.save(OsStr::new("keypair"), &[1]));
        assert!(!t.0.exists());
    }

    #[test]
    fn unpaired_access_switch() {
        let t = TmpDir::new();
        let store = FileStore::new(t.0.clone());
        assert!(store.ensure());
        assert!(store.unpaired_access());
        for (content, on) in [
            (&b"0"[..], false),
            (b"0\n", false),
            (b" \t\r\n\x0b\x0c0", false),
            (b"00", false),
            (b"1", true),
            (b"off", true),
            (b"", true),
            (b" \n", true),
            (b"\x000", true),
        ] {
            fs::write(t.0.join(UNPAIRED_ACCESS), content).unwrap();
            assert_eq!(store.unpaired_access(), on, "{content:?}");
        }
    }

    #[test]
    fn memory_store_keeps_blobs_and_the_switch() {
        let store = MemoryStore::new();
        assert!(store.ensure());
        assert!(store.load(OsStr::new("keypair")).is_none());
        assert!(store.save(OsStr::new("keypair"), &[1, 2, 3]));
        assert_eq!(store.load(OsStr::new("keypair")), Some(vec![1, 2, 3]));
        assert!(store.unpaired_access());
        assert!(
            !MemoryStore::new()
                .with_unpaired_access(false)
                .unpaired_access()
        );
    }
}
