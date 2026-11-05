//! Redacted secrets and token files for first-party Ricardo services.
//!
//! [`Secret`] and [`Token`] keep their contents in a [`Zeroize`]ing buffer,
//! print `***` from `Debug`, and only compare through [`subtle`]'s
//! constant-time equality. [`load_or_create`] owns the on-disk contract used
//! by the station daemons: a `0600` file inside a `0700` directory.

use std::fmt;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A secret string that never leaks through `Debug`, `Display` or `Clone`
/// (cloning copies the value into another zeroizing buffer).
///
/// Call [`Secret::expose`] at the exact place a protocol needs the plaintext.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Secret {
    value: String,
}

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    /// Borrow the plaintext. Use it as narrowly as possible.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.value
    }

    fn as_bytes(&self) -> &[u8] {
        self.value.as_bytes()
    }

    /// Constant-time comparison against an untrusted candidate.
    #[must_use]
    pub fn ct_eq(&self, candidate: impl AsRef<[u8]>) -> bool {
        self.value.as_bytes().ct_eq(candidate.as_ref()).into()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Secret(***)")
    }
}

impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        self.ct_eq(other.value.as_bytes())
    }
}

impl Eq for Secret {}

/// A protocol token (HTTP bearer, device token) with the same redaction and
/// constant-time comparison rules as [`Secret`].
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Token {
    value: Secret,
}

impl Token {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: Secret::new(value),
        }
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        self.value.expose()
    }

    #[must_use]
    pub fn ct_eq(&self, candidate: impl AsRef<[u8]>) -> bool {
        self.value.ct_eq(candidate)
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Token(***)")
    }
}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Eq for Token {}

/// Read a token file or create it with `generate`.
///
/// The file is trimmed; an empty result is treated as missing. A generated
/// value is written as `value\n` with mode `0600`, and the parent directory is
/// created with mode `0700`.
pub fn load_or_create<G>(path: &Path, generate: G) -> io::Result<Secret>
where
    G: FnOnce() -> io::Result<Secret>,
{
    load_or_create_with(path, |value| Some(value.to_string()), generate)
}

/// Like [`load_or_create`], but lets the caller normalise the file contents
/// before accepting them. Returning `None` (or an empty string) regenerates
/// the token.
pub fn load_or_create_with<R, G>(path: &Path, normalize: R, generate: G) -> io::Result<Secret>
where
    R: FnOnce(&str) -> Option<String>,
    G: FnOnce() -> io::Result<Secret>,
{
    if let Ok(raw) = fs::read_to_string(path)
        && let Some(value) = normalize(raw.trim())
        && !value.is_empty()
    {
        ensure_private_directory(path)?;
        tighten_permissions(path)?;
        return Ok(Secret::new(value));
    }

    let secret = generate()?;
    ensure_private_directory(path)?;
    write_secret(path, &secret)?;
    Ok(secret)
}

/// Load or generate a hex token of `bytes` random bytes (`2 * bytes` chars).
pub fn load_or_create_hex(path: &Path, bytes: usize) -> io::Result<Token> {
    load_or_create(path, || hex(bytes)).map(|secret| Token::new(secret.expose()))
}

/// Generate a decimal token with a non-zero leading digit.
pub fn numeric(digits: usize) -> io::Result<Secret> {
    if digits == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a numeric token needs at least one digit",
        ));
    }
    let mut value = String::with_capacity(digits);
    value.push(char::from(b'0' + uniform_below(9) as u8 + 1));
    for _ in 1..digits {
        value.push(char::from(b'0' + uniform_below(10) as u8));
    }
    Ok(Secret::new(value))
}

/// Generate a lower-case hex token from `bytes` random bytes.
pub fn hex(bytes: usize) -> io::Result<Secret> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer)
        .map_err(|error| io::Error::other(format!("getrandom: {error}")))?;
    Ok(Secret::new(hex::encode(buffer)))
}

/// Write `secret` to `path` as `secret\n` with mode `0600`.
fn write_secret(path: &Path, secret: &Secret) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(secret.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

fn ensure_private_directory(path: &Path) -> io::Result<()> {
    let directory = directory_of(path);
    let mut builder = DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
}

fn tighten_permissions(path: &Path) -> io::Result<()> {
    let mode = fs::metadata(path)?.permissions().mode() & 0o777;
    if mode != 0o600 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn directory_of(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

/// Uniform integer in `0..bound` without modulo bias.
fn uniform_below(bound: u32) -> u32 {
    debug_assert!(bound > 0);
    let zone = u32::MAX - (u32::MAX % bound);
    loop {
        let mut bytes = [0u8; 4];
        // getrandom failing is a broken platform, not a recoverable condition
        // for a token that gates physical provisioning.
        getrandom::fill(&mut bytes).expect("getrandom");
        let value = u32::from_le_bytes(bytes);
        if value < zone {
            return value % bound;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "ricardo-secret-{tag}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).expect("stat").permissions().mode() & 0o777
    }

    #[test]
    fn debug_and_display_never_show_the_value() {
        let secret = Secret::new("123456");
        assert_eq!(format!("{secret:?}"), "Secret(***)");
        assert!(!format!("{secret:?}").contains("123456"));
        let token = Token::new("0123456789abcdef");
        assert_eq!(format!("{token:?}"), "Token(***)");
    }

    #[test]
    fn comparison_is_constant_time_and_length_safe() {
        let secret = Secret::new("123456");
        assert!(secret.ct_eq("123456"));
        assert!(!secret.ct_eq("1234567"));
        assert!(!secret.ct_eq("123457"));
        assert_eq!(secret, Secret::new("123456"));
        assert_ne!(secret, Secret::new("123457"));
    }

    #[test]
    fn load_or_create_writes_private_file_and_directory() {
        let dir = temp_dir("create");
        let path = dir.join("nested").join("token");
        let secret = load_or_create(&path, || numeric(6)).expect("create");
        assert_eq!(secret.expose().len(), 6);
        assert!(secret.expose().chars().all(|c| c.is_ascii_digit()));
        assert!(!secret.expose().starts_with('0'));
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().expect("parent")), 0o700);
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            format!("{}\n", secret.expose())
        );

        // A second call reuses the file, trims it and repairs its mode.
        fs::write(&path, " 654321 \n").expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");
        let reused = load_or_create(&path, || numeric(6)).expect("reuse");
        assert_eq!(reused.expose(), "654321");
        assert_eq!(mode(&path), 0o600);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_contents_regenerate() {
        let dir = temp_dir("invalid");
        let path = dir.join("token");
        fs::write(&path, "not digits\n").expect("write");
        let secret = load_or_create_with(
            &path,
            |value| {
                let digits: String = value.chars().filter(char::is_ascii_digit).collect();
                (!digits.is_empty()).then_some(digits)
            },
            || numeric(6),
        )
        .expect("regenerate");
        assert_eq!(secret.expose().len(), 6);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn numeric_and_hex_shapes() {
        assert_eq!(
            numeric(0).expect_err("zero digits").kind(),
            io::ErrorKind::InvalidInput
        );
        for digits in [1, 6, 12] {
            let value = numeric(digits).expect("numeric");
            assert_eq!(value.expose().len(), digits);
        }
        let token = hex(32).expect("hex");
        assert_eq!(token.expose().len(), 64);
        assert!(token.expose().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn load_or_create_hex_token() {
        let dir = temp_dir("hex");
        let path = dir.join("control-token");
        let token = load_or_create_hex(&path, 32).expect("create");
        assert_eq!(token.expose().len(), 64);
        assert_eq!(mode(&path), 0o600);
        let again = load_or_create_hex(&path, 32).expect("reuse");
        assert_eq!(token.expose(), again.expose());
        let _ = fs::remove_dir_all(dir);
    }
}
