//! Telegram Bot API `sendMessage` delivery for the home services.
//!
//! The crate stays project-agnostic: it knows one bot token and one chat id,
//! sends HTML messages, and splits long text at the Bot API limit. Reports,
//! redaction and recipients belong to the caller.
//!
//! [`chunk`] mirrors the Go gateway's `telegram.Chunk`: the title and body are
//! HTML-escaped, joined, and split into messages of at most [`MAX_MESSAGE_RUNES`]
//! runes. Splitting prefers a newline boundary so words and tags stay intact; a
//! single over-long line is hard-wrapped.
//!
//! [`Error::kind`] is [`ErrorKind::Transient`] for network failures, timeouts,
//! `408`, `425`, `429` and `5xx`, and [`ErrorKind::Permanent`] for every other
//! `4xx`, an `ok:false` answer and a malformed answer.

use std::fmt;

use home_secret::Token;
use serde::Deserialize;

/// The Bot API limit for one message, in runes.
pub const MAX_MESSAGE_RUNES: usize = 4096;

const MAX_RESPONSE_BYTES: usize = 64 << 10;

/// How a failed call may be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// A network failure, timeout, throttling or a `5xx`: retrying later may
    /// succeed.
    Transient,
    /// A `4xx`, a rejected message or an invalid argument: retrying the same
    /// call cannot succeed.
    Permanent,
}

/// One Bot API call failure. It never contains the bot token or a message body.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{detail}")]
    Invalid { detail: &'static str },
    #[error("telegram sendMessage returned HTTP {status}")]
    Status { status: u16 },
    #[error("telegram sendMessage failed: {description}")]
    Api { description: String },
    #[error("telegram sendMessage: request failed")]
    Transport {
        #[source]
        source: reqwest::Error,
    },
    #[error("telegram response is not JSON")]
    Decode {
        #[source]
        source: serde_json::Error,
    },
}

impl Error {
    /// Whether a retry may succeed.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Transport { .. } => ErrorKind::Transient,
            Self::Status { status } => match *status {
                408 | 425 | 429 => ErrorKind::Transient,
                status if status >= 500 => ErrorKind::Transient,
                _ => ErrorKind::Permanent,
            },
            Self::Invalid { .. } | Self::Api { .. } | Self::Decode { .. } => ErrorKind::Permanent,
        }
    }

    /// Convenience predicate for a worker loop.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        self.kind() == ErrorKind::Transient
    }
}

/// A sender for one bot and one fixed chat.
#[derive(Clone)]
pub struct Sender {
    http: reqwest::Client,
    api_url: String,
    token: Token,
    chat_id: String,
}

impl fmt::Debug for Sender {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("home_telegram::Sender")
            .field("api_url", &self.api_url)
            .field("token", &self.token)
            .field("chat_id", &self.chat_id)
            .finish()
    }
}

impl Sender {
    /// Build a sender. `api_url` is the Bot API root (`https://api.telegram.org`
    /// or a local fake); `token` and `chat_id` are trimmed of whitespace.
    pub fn new(
        api_url: impl Into<String>,
        token: Token,
        chat_id: impl Into<String>,
        timeout: std::time::Duration,
    ) -> Result<Self, Error> {
        // The workspace `reqwest` has no default crypto provider; install one
        // before the first client. A provider installed earlier is kept.
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|source| Error::Transport { source })?;
        Ok(Self {
            http,
            api_url: api_url.into().trim().trim_end_matches('/').to_owned(),
            token,
            chat_id: chat_id.into().trim().to_owned(),
        })
    }

    /// Escape and deliver a title + text report in as many chunks as needed.
    pub async fn send(&self, title: &str, text: &str) -> Result<(), Error> {
        if self.token.expose().is_empty() || self.chat_id.is_empty() {
            return Err(Error::Invalid {
                detail: "telegram token and chat id are required",
            });
        }
        for part in chunk(title, text, MAX_MESSAGE_RUNES) {
            self.send_message(&part).await?;
        }
        Ok(())
    }

    /// Send one already-formatted HTML message.
    ///
    /// The caller is responsible for escaping user text and for staying under
    /// [`MAX_MESSAGE_RUNES`].
    pub async fn send_message(&self, text: &str) -> Result<(), Error> {
        if self.token.expose().is_empty() || self.chat_id.is_empty() {
            return Err(Error::Invalid {
                detail: "telegram token and chat id are required",
            });
        }
        let endpoint = format!("{}/bot{}/sendMessage", self.api_url, self.token.expose());
        let response = self
            .http
            .post(endpoint)
            .form(&[
                ("chat_id", self.chat_id.clone()),
                ("text", text.to_owned()),
                ("parse_mode", "HTML".to_owned()),
                ("disable_web_page_preview", "true".to_owned()),
            ])
            .send()
            .await
            .map_err(|source| Error::Transport { source })?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .await
            .map_err(|source| Error::Transport { source })?;
        if status != 200 {
            return Err(Error::Status { status });
        }
        if body.len() > MAX_RESPONSE_BYTES {
            return Err(Error::Invalid {
                detail: "telegram response is too large",
            });
        }
        let decoded: SendResponse =
            serde_json::from_slice(&body).map_err(|source| Error::Decode { source })?;
        if !decoded.ok {
            return Err(Error::Api {
                description: decoded.description,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct SendResponse {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    description: String,
}

/// Escape a title and text as Telegram HTML and split the result into messages
/// no longer than `limit` runes.
///
/// `limit` outside `1..=MAX_MESSAGE_RUNES` becomes [`MAX_MESSAGE_RUNES`].
#[must_use]
pub fn chunk(title: &str, text: &str, limit: usize) -> Vec<String> {
    let limit = if limit == 0 || limit > MAX_MESSAGE_RUNES {
        MAX_MESSAGE_RUNES
    } else {
        limit
    };
    let title = title.trim();
    let mut full = String::new();
    if !title.is_empty() {
        full.push_str("<b>");
        full.push_str(&escape_html(title));
        full.push_str("</b>");
    }
    let body = escape_html(text.trim_end_matches('\n'));
    if !body.is_empty() {
        if !full.is_empty() {
            full.push_str("\n\n");
        }
        full.push_str(&body);
    }
    if full.is_empty() {
        return Vec::new();
    }
    split(&full, limit)
}

fn split(full: &str, limit: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut remaining = full;
    while remaining.chars().count() > limit {
        let mut cut = rune_offset(remaining, limit);
        // Prefer the last newline before the limit so HTML tags and words stay
        // intact; HTML entities introduce no raw '<' so this is safe.
        if let Some(index) = remaining[..cut].rfind('\n')
            && index > 0
        {
            cut = index + 1;
        }
        let part = remaining[..cut].trim_end_matches(['\n', ' ']);
        if !part.is_empty() {
            chunks.push(part.to_owned());
        }
        remaining = remaining[cut..].trim_start_matches('\n');
    }
    if !remaining.trim().is_empty() {
        chunks.push(remaining.to_owned());
    }
    chunks
}

fn rune_offset(text: &str, runes: usize) -> usize {
    text.char_indices()
        .nth(runes)
        .map_or(text.len(), |(index, _)| index)
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '\'' => escaped.push_str("&#39;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&#34;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Bytes;
    use axum::http::StatusCode;
    use axum::routing::post;

    use super::*;

    type Bodies = Arc<std::sync::Mutex<Vec<(String, HashMap<String, String>)>>>;

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}")
    }

    #[test]
    fn chunk_escapes_html() {
        let chunks = chunk(
            "Отчёт <script>",
            "Строка <b>не тег</b> & далее",
            MAX_MESSAGE_RUNES,
        );
        assert_eq!(chunks.len(), 1);
        let got = &chunks[0];
        assert!(got.contains("<b>Отчёт &lt;script&gt;</b>"));
        assert!(got.contains("&lt;b&gt;не тег&lt;/b&gt; &amp; далее"));
        assert!(!got.contains("<script>"));
    }

    #[test]
    fn chunk_splits_at_limit_on_line_boundaries() {
        let lines = vec!["строка лога с текстом"; 40];
        let chunks = chunk("Заголовок", &lines.join("\n"), 100);
        assert!(chunks.len() >= 2);
        for part in &chunks {
            assert!(part.chars().count() <= 100, "chunk too long: {part}");
        }
        let joined = chunks.join("\n");
        assert_eq!(joined.matches("строка лога с текстом").count(), 40);
    }

    #[test]
    fn chunk_empty_and_title_only() {
        assert!(chunk("", "   ", 100).is_empty());
        assert_eq!(
            chunk("Только заголовок", "", 100),
            ["<b>Только заголовок</b>"]
        );
        assert_eq!(chunk("", "  body\n", 100), ["  body"]);
    }

    #[tokio::test]
    async fn send_posts_every_chunk_with_html_mode() {
        let bodies = Arc::new(std::sync::Mutex::new(Vec::<(
            String,
            HashMap<String, String>,
        )>::new()));
        let app =
            Router::new()
                .route(
                    "/botTOKEN/sendMessage",
                    post(
                        |axum::extract::State(bodies): axum::extract::State<Bodies>,
                         body: Bytes| async move {
                            let form: HashMap<String, String> =
                                serde_urlencoded::from_bytes(&body).expect("form");
                            bodies
                                .lock()
                                .expect("lock")
                                .push(("sendMessage".to_owned(), form));
                            axum::Json(serde_json::json!({"ok": true}))
                        },
                    ),
                )
                .with_state(bodies.clone());
        let base = serve(app).await;
        let sender = Sender::new(
            base,
            Token::new("TOKEN"),
            "-100123",
            std::time::Duration::from_secs(5),
        )
        .expect("sender");
        let lines = vec![
            "достаточно длинная строка для разбиения отчёта";
            200
        ];
        sender
            .send("Заголовок", &lines.join("\n"))
            .await
            .expect("send");
        let bodies = bodies.lock().expect("lock");
        assert!(bodies.len() >= 2);
        for (_, form) in bodies.iter() {
            assert_eq!(form["chat_id"], "-100123");
            assert_eq!(form["parse_mode"], "HTML");
            assert_eq!(form["disable_web_page_preview"], "true");
        }
    }

    #[tokio::test]
    async fn send_errors_are_classified() {
        let sender = Sender::new(
            "http://example",
            Token::new(""),
            "1",
            std::time::Duration::from_secs(1),
        )
        .expect("sender");
        let error = sender.send("t", "x").await.expect_err("empty token");
        assert_eq!(error.kind(), ErrorKind::Permanent);

        let app = Router::new().route(
            "/botTOKEN/sendMessage",
            post(|| async {
                axum::Json(serde_json::json!({"ok": false, "description": "chat not found"}))
            }),
        );
        let base = serve(app).await;
        let sender = Sender::new(
            base,
            Token::new("TOKEN"),
            "1",
            std::time::Duration::from_secs(1),
        )
        .expect("sender");
        let error = sender.send("t", "x").await.expect_err("ok=false");
        assert_eq!(error.kind(), ErrorKind::Permanent);

        let app = Router::new().route(
            "/botTOKEN/sendMessage",
            post(|| async { StatusCode::TOO_MANY_REQUESTS }),
        );
        let base = serve(app).await;
        let sender = Sender::new(
            base,
            Token::new("TOKEN"),
            "1",
            std::time::Duration::from_secs(1),
        )
        .expect("sender");
        let error = sender.send_message("x").await.expect_err("HTTP 429");
        assert_eq!(error.kind(), ErrorKind::Transient);
    }
}
