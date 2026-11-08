//! Конфигурация звонка: endpoint intercom, device token и CA.
//!
//! Модуль выключен (см. [`crate::spawn`]) без URL и токена: станция не открывает исходящих
//! соединений и не рекламирует звонок. Токен читается только из файла с правами `0600`;
//! сгенерировать его станция не может — endpoint регистрирует владелец.

#[cfg(test)]
use std::io;
use std::path::Path;

use home_secret::Token;

/// Ошибка чтения конфигурации. Никогда не содержит путь к токену и его значение.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("call: endpoint URL is not http(s)")]
    InvalidUrl,
    #[error("call: token file is missing or not readable")]
    Token,
    #[error("call: token file permissions are too open")]
    TokenPermissions,
    #[error("call: CA bundle is not readable")]
    Ca,
}

/// Всё, что нужно клиенту сигналинга.
#[derive(Clone)]
pub struct CallConfig {
    /// База сервиса без завершающего `/`, например `https://intercom.example/v1`.
    endpoint: String,
    token: Token,
    /// PEM-бандл приватного CA; иначе системные корни transports.
    ca_pem: Option<Vec<u8>>,
}

impl std::fmt::Debug for CallConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallConfig")
            .field("endpoint", &self.endpoint)
            .field("token", &self.token)
            .field("ca", &self.ca_pem.as_ref().map(Vec::len))
            .finish()
    }
}

/// Каталог провижининга звонка на станции (рядом с `state.conf`).
pub const CALL_DIR: &str = "/data/pulsar";
/// URL сервиса: первая строка файла. Нет файла — звонок выключен.
pub const URL_FILE: &str = "intercom_url";
/// Device token станции (`0600`).
pub const TOKEN_FILE: &str = "intercom_token";
/// PEM приватного CA (необязателен: без него транспорт использует свои корни).
pub const CA_FILE: &str = "intercom_ca.pem";

impl CallConfig {
    /// Конфигурация из каталога провижининга ([`CALL_DIR`]): URL, токен, CA.
    ///
    /// Отдельные файлы, а не `state.conf`: кодек `state.conf` побайтово совместим с C++,
    /// и посторонние ключи ломали бы эталонные фикстуры.
    pub fn load_dir(dir: &Path) -> Result<Option<Self>, ConfigError> {
        let url = match std::fs::read_to_string(dir.join(URL_FILE)) {
            Ok(text) => text.lines().next().unwrap_or_default().trim().to_owned(),
            Err(_) => return Ok(None),
        };
        let token = dir.join(TOKEN_FILE);
        let ca = dir.join(CA_FILE);
        let ca_path = ca.exists().then_some(ca.as_path());
        Self::load(&url, &token, ca_path)
    }

    /// `url` — endpoint intercom; пусто — модуль выключен.
    ///
    /// `token_path` обязателен: файл должен существовать и быть `0600`.
    pub fn load(
        url: &str,
        token_path: &Path,
        ca_path: Option<&Path>,
    ) -> Result<Option<Self>, ConfigError> {
        let endpoint = url.trim();
        if endpoint.is_empty() {
            return Ok(None);
        }
        let endpoint = endpoint.trim_end_matches('/').to_owned();
        if !(endpoint.starts_with("https://") || endpoint.starts_with("http://")) {
            return Err(ConfigError::InvalidUrl);
        }
        let token = read_token(token_path)?;
        let ca_pem = match ca_path {
            Some(path) => Some(std::fs::read(path).map_err(|_| ConfigError::Ca)?),
            None => None,
        };
        Ok(Some(Self {
            endpoint,
            token,
            ca_pem,
        }))
    }

    /// База для `ClientConfig`: без `/home.v1.IntercomService`, если путь уже указан.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        self.endpoint
            .strip_suffix("/home.v1.IntercomService")
            .unwrap_or(&self.endpoint)
    }

    #[must_use]
    pub fn token(&self) -> &Token {
        &self.token
    }

    #[must_use]
    pub fn ca_pem(&self) -> Option<&[u8]> {
        self.ca_pem.as_deref()
    }
}

fn read_token(path: &Path) -> Result<Token, ConfigError> {
    let meta = std::fs::metadata(path).map_err(|_| ConfigError::Token)?;
    if meta.len() == 0 || meta.len() > 4096 {
        return Err(ConfigError::Token);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        // The device token identifies the station: group/other read is a leak.
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(ConfigError::TokenPermissions);
        }
    }
    let raw = std::fs::read_to_string(path).map_err(|_| ConfigError::Token)?;
    let value = raw.trim();
    if value.is_empty() {
        return Err(ConfigError::Token);
    }
    Ok(Token::new(value))
}

/// Записать токен (провижининг на станции вне этого модуля): `0600`, атомарно.
///
/// Нужен тестам; встроенного провижининга токена у станции нет — endpoint регистрирует
/// владелец, значение не логируется.
#[cfg(test)]
pub fn write_token(path: &Path, value: &str) -> io::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write as _;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        #[cfg(unix)]
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let tmp = path.with_extension("tmp");
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(value.trim().as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)?;
    #[cfg(unix)]
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Temp(PathBuf);

    impl Temp {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "pulsar-call-{}-{}-{}",
                name,
                std::process::id(),
                pulsar_core::now_ms()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn empty_url_disables_the_module() {
        let dir = Temp::new("disabled");
        let token = dir.0.join("token");
        assert!(CallConfig::load("  ", &token, None).unwrap().is_none());
    }

    #[test]
    fn a_plain_token_file_is_accepted_and_the_service_suffix_is_stripped() {
        let dir = Temp::new("ok");
        let token = dir.0.join("token");
        write_token(&token, "secret").unwrap();
        let cfg = CallConfig::load(
            "https://intercom.example/v1/home.v1.IntercomService/",
            &token,
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(cfg.endpoint(), "https://intercom.example/v1");
        assert_eq!(cfg.token().expose(), "secret");
    }

    #[test]
    fn an_open_token_file_is_refused() {
        let dir = Temp::new("perm");
        let token = dir.0.join("token");
        write_token(&token, "secret").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(matches!(
                CallConfig::load("https://intercom.example/", &token, None),
                Err(ConfigError::TokenPermissions)
            ));
        }
    }

    #[test]
    fn a_missing_token_file_is_refused() {
        let dir = Temp::new("missing");
        assert!(matches!(
            CallConfig::load("https://intercom.example/", &dir.0.join("nope"), None),
            Err(ConfigError::Token)
        ));
    }

    #[test]
    fn a_non_http_url_is_refused() {
        let dir = Temp::new("url");
        let token = dir.0.join("token");
        write_token(&token, "secret").unwrap();
        assert!(matches!(
            CallConfig::load("intercom.example", &token, None),
            Err(ConfigError::InvalidUrl)
        ));
    }
}
