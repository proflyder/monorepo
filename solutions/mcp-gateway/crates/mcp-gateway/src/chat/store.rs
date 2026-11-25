use std::path::Path;
use std::sync::{Mutex, PoisonError};

use rusqlite::{Connection, OptionalExtension as _, Row, params};

use super::alerts::AlertCard;
use super::model::{
    Attachment, ErrorKey, Kind, Message, PREVIEW_CHARS, Preview, Quote, Role, Session,
    SessionPatch, Status,
};

pub const QUOTE_CHARS: usize = 200;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS chat_messages (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    version INTEGER NOT NULL,
    client_id TEXT UNIQUE,
    role TEXT NOT NULL,
    kind TEXT NOT NULL,
    text TEXT NOT NULL,
    reply_to TEXT,
    request_id TEXT,
    status TEXT NOT NULL,
    error TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    attachments TEXT,
    transcript TEXT,
    alert TEXT,
    alert_key TEXT
);
CREATE INDEX IF NOT EXISTS chat_messages_version ON chat_messages(version);
CREATE INDEX IF NOT EXISTS chat_messages_request ON chat_messages(request_id);
CREATE TABLE IF NOT EXISTS chat_sessions (
    id TEXT PRIMARY KEY,
    title TEXT,
    pinned INTEGER NOT NULL DEFAULT 0,
    archived INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
";

const ALERT_INDEX: &str =
    "CREATE UNIQUE INDEX IF NOT EXISTS chat_messages_alert ON chat_messages(alert_key);
CREATE INDEX IF NOT EXISTS chat_messages_session ON chat_messages(session_id, seq);";

const COLUMNS: &str = "m.seq, m.id, m.version, m.client_id, m.role, m.kind, m.text, m.status, \
     m.error, m.created_at, m.updated_at, q.id, q.role, q.text, m.attachments, m.transcript, m.alert, m.session_id";

const ADDED_COLUMNS: [(&str, &str); 5] = [
    ("attachments", "TEXT"),
    ("transcript", "TEXT"),
    ("alert", "TEXT"),
    ("alert_key", "TEXT"),
    ("session_id", "TEXT"),
];

const SESSION_COLUMNS: &str = "s.id, s.title, s.pinned, s.archived, s.created_at, s.updated_at, \
     m.role, m.kind, m.text, m.transcript, m.status, m.created_at";

const SESSION_FROM: &str = "chat_sessions s LEFT JOIN chat_messages m ON m.seq = \
     (SELECT MAX(seq) FROM chat_messages WHERE session_id = s.id)";

const SESSION_ORDER: &str =
    "ORDER BY s.pinned DESC, MAX(s.updated_at, COALESCE(m.created_at, 0)) DESC";

const FROM: &str = "chat_messages m LEFT JOIN chat_messages q ON q.id = m.reply_to";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("chat storage: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("chat storage: quoted message not found")]
    ReplyNotFound,
    #[error("chat storage: message not found")]
    NotFound,
    #[error("chat storage: session not found")]
    SessionNotFound,
}

pub struct NewTurn<'a> {
    pub session_id: &'a str,
    pub client_id: &'a str,
    pub text: &'a str,
    pub kind: Kind,
    pub attachments: &'a [Attachment],
    pub reply_to: Option<&'a str>,
    pub user_id: String,
    pub reply_id: String,
    pub now: i64,
}

pub struct Update<'a> {
    pub text: Option<&'a str>,
    pub status: Status,
    pub error: Option<ErrorKey>,
    pub now: i64,
}

pub struct Store {
    connection: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            let _created = std::fs::create_dir_all(parent);
        }
        Self::from_connection(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self, StoreError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self, StoreError> {
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.execute_batch(SCHEMA)?;
        migrate(&connection)?;
        connection.execute_batch(ALERT_INDEX)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn with<T>(
        &self,
        action: impl FnOnce(&mut Connection) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        action(&mut connection)
    }

    pub fn cursor(&self) -> Result<i64, StoreError> {
        self.with(|connection| {
            Ok(connection.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM chat_messages",
                [],
                |row| row.get(0),
            )?)
        })
    }

    pub fn insert_turn(&self, turn: &NewTurn<'_>) -> Result<(Message, Message, bool), StoreError> {
        self.with(|connection| insert_turn(connection, turn))
    }

    pub fn get(&self, id: &str) -> Result<Option<Message>, StoreError> {
        self.with(|connection| fetch(connection, id))
    }

    pub fn request_of(&self, reply_id: &str) -> Result<Option<Message>, StoreError> {
        self.with(|connection| {
            let request: Option<Option<String>> = connection
                .query_row(
                    "SELECT request_id FROM chat_messages WHERE id = ?1",
                    params![reply_id],
                    |row| row.get(0),
                )
                .optional()?;
            request
                .flatten()
                .map_or(Ok(None), |id: String| fetch(connection, &id))
        })
    }

    pub fn page_before(
        &self,
        scope: &Scope<'_>,
        before: Option<i64>,
        limit: usize,
    ) -> Result<(Vec<Message>, bool), StoreError> {
        let bound = before.unwrap_or(i64::MAX);
        let fetch_limit = i64::try_from(limit + 1).unwrap_or(i64::MAX);
        let mut rows = self.with(|connection| {
            query(
                connection,
                &format!(
                    "SELECT {COLUMNS} FROM {FROM} WHERE m.seq < ?1 AND {SCOPED} ORDER BY m.seq DESC LIMIT ?2"
                ),
                bound,
                fetch_limit,
                scope,
            )
        })?;
        let has_more = rows.len() > limit;
        rows.truncate(limit);
        rows.reverse();
        Ok((rows, has_more))
    }

    pub fn changed_after(
        &self,
        scope: &Scope<'_>,
        version: i64,
        limit: usize,
    ) -> Result<Vec<Message>, StoreError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with(|connection| {
            query(
                connection,
                &format!(
                    "SELECT {COLUMNS} FROM {FROM} WHERE m.version > ?1 AND {SCOPED} ORDER BY m.version ASC LIMIT ?2"
                ),
                version,
                limit,
                scope,
            )
        })
    }

    pub fn adopt_legacy(&self, session_id: &str, title: &str, now: i64) -> Result<(), StoreError> {
        self.with(|connection| {
            let transaction = connection.transaction()?;
            transaction.execute(
                "INSERT OR IGNORE INTO chat_sessions (id, title, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?3)",
                params![session_id, title, now],
            )?;
            transaction.execute(
                "UPDATE chat_messages SET session_id = ?1 WHERE session_id IS NULL",
                params![session_id],
            )?;
            transaction.commit()?;
            Ok(())
        })
    }

    pub fn sessions(&self, archived: bool) -> Result<Vec<Session>, StoreError> {
        self.with(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {SESSION_COLUMNS} FROM {SESSION_FROM} WHERE s.archived = ?1 {SESSION_ORDER}"
            ))?;
            let rows = statement
                .query_map(params![i64::from(archived)], read_session)?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        })
    }

    pub fn session(&self, id: &str) -> Result<Option<Session>, StoreError> {
        self.with(|connection| fetch_session(connection, id))
    }

    pub fn create_session(
        &self,
        id: &str,
        title: Option<&str>,
        now: i64,
    ) -> Result<Session, StoreError> {
        self.with(|connection| {
            connection.execute(
                "INSERT INTO chat_sessions (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
                params![id, title, now],
            )?;
            fetch_session(connection, id)?.ok_or(StoreError::SessionNotFound)
        })
    }

    pub fn patch_session(
        &self,
        id: &str,
        patch: &SessionPatch,
        now: i64,
    ) -> Result<Session, StoreError> {
        self.with(|connection| {
            let changed = connection.execute(
                "UPDATE chat_sessions SET title = COALESCE(?2, title), \
                 pinned = COALESCE(?3, pinned), archived = COALESCE(?4, archived), \
                 updated_at = ?5 WHERE id = ?1",
                params![
                    id,
                    patch.title,
                    patch.pinned.map(i64::from),
                    patch.archived.map(i64::from),
                    now
                ],
            )?;
            if changed == 0 {
                return Err(StoreError::SessionNotFound);
            }
            fetch_session(connection, id)?.ok_or(StoreError::SessionNotFound)
        })
    }

    pub fn title_if_missing(&self, id: &str, title: &str) -> Result<bool, StoreError> {
        self.with(|connection| {
            let changed = connection.execute(
                "UPDATE chat_sessions SET title = ?2 WHERE id = ?1 AND (title IS NULL OR title = '')",
                params![id, title],
            )?;
            Ok(changed > 0)
        })
    }

    pub fn delete_session(&self, id: &str) -> Result<Vec<Attachment>, StoreError> {
        self.with(|connection| {
            let transaction = connection.transaction()?;
            let attachments = session_attachments(&transaction, id)?;
            transaction.execute(
                "DELETE FROM chat_messages WHERE session_id = ?1",
                params![id],
            )?;
            let removed =
                transaction.execute("DELETE FROM chat_sessions WHERE id = ?1", params![id])?;
            if removed == 0 {
                return Err(StoreError::SessionNotFound);
            }
            transaction.commit()?;
            Ok(attachments)
        })
    }

    pub fn first_user_text(&self, session_id: &str) -> Result<Option<String>, StoreError> {
        self.with(|connection| {
            Ok(connection
                .query_row(
                    "SELECT COALESCE(NULLIF(text, ''), transcript) FROM chat_messages \
                     WHERE session_id = ?1 AND role = 'user' ORDER BY seq ASC LIMIT 1",
                    params![session_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten())
        })
    }

    pub fn upsert_alert(
        &self,
        session_id: &str,
        id: &str,
        card: &AlertCard,
        text: &str,
        now: i64,
    ) -> Result<(Message, bool), StoreError> {
        let encoded = serde_json::to_string(card).unwrap_or_default();
        let key = card.key();
        self.with(|connection| {
            let transaction = connection.transaction()?;
            let existing: Option<String> = transaction
                .query_row(
                    "SELECT id FROM chat_messages WHERE alert_key = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .optional()?;
            let version = next_version(&transaction)?;
            let target = existing.clone().unwrap_or_else(|| id.to_owned());
            match existing {
                Some(_) => transaction.execute(
                    "UPDATE chat_messages SET text = ?2, alert = ?3, version = ?4, updated_at = ?5 \
                     WHERE id = ?1",
                    params![target, text, encoded, version, now],
                )?,
                None => transaction.execute(
                    "INSERT INTO chat_messages (id, version, role, kind, text, status, created_at, \
                     updated_at, alert, alert_key, session_id) \
                     VALUES (?1, ?2, 'system', 'alert', ?3, 'done', ?4, ?4, ?5, ?6, ?7)",
                    params![target, version, text, now, encoded, key, session_id],
                )?,
            };
            let message = fetch(&transaction, &target)?.ok_or(StoreError::NotFound)?;
            transaction.commit()?;
            Ok((message, existing.is_none()))
        })
    }

    pub fn set_transcript(
        &self,
        id: &str,
        transcript: &str,
        now: i64,
    ) -> Result<Message, StoreError> {
        self.with(|connection| {
            let version = next_version(connection)?;
            connection.execute(
                "UPDATE chat_messages SET transcript = ?2, version = ?3, updated_at = ?4 WHERE id = ?1",
                params![id, transcript, version, now],
            )?;
            fetch(connection, id)?.ok_or(StoreError::NotFound)
        })
    }

    pub fn update(&self, id: &str, update: &Update<'_>) -> Result<Message, StoreError> {
        self.with(|connection| update_message(connection, id, update))
    }

    pub fn recover(&self, now: i64) -> Result<(Vec<String>, Vec<Message>), StoreError> {
        let interrupted = self.with(|connection| ids(connection, "streaming"))?;
        let failed = interrupted
            .iter()
            .map(|id| {
                self.update(
                    id,
                    &Update {
                        text: None,
                        status: Status::Failed,
                        error: Some(ErrorKey::ChatInterrupted),
                        now,
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let pending = self.with(|connection| ids(connection, "pending"))?;
        Ok((pending, failed))
    }
}

fn ids(connection: &Connection, status: &str) -> Result<Vec<String>, StoreError> {
    let mut statement =
        connection.prepare("SELECT id FROM chat_messages WHERE status = ?1 ORDER BY seq ASC")?;
    let rows = statement
        .query_map(params![status], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

const SCOPED: &str = "(?3 = 1 OR m.kind <> 'alert') AND (?4 IS NULL OR m.session_id = ?4)";

pub struct Scope<'a> {
    pub session: Option<&'a str>,
    pub include_alerts: bool,
}

fn query(
    connection: &Connection,
    sql: &str,
    bound: i64,
    limit: i64,
    scope: &Scope<'_>,
) -> Result<Vec<Message>, StoreError> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement
        .query_map(
            params![bound, limit, i64::from(scope.include_alerts), scope.session],
            read_message,
        )?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

fn session_attachments(connection: &Connection, id: &str) -> Result<Vec<Attachment>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT attachments FROM chat_messages WHERE session_id = ?1 AND attachments IS NOT NULL",
    )?;
    let encoded: Vec<String> = statement
        .query_map(params![id], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(encoded
        .iter()
        .filter_map(|raw| serde_json::from_str::<Vec<Attachment>>(raw).ok())
        .flatten()
        .collect())
}

fn fetch_session(connection: &Connection, id: &str) -> Result<Option<Session>, StoreError> {
    Ok(connection
        .query_row(
            &format!("SELECT {SESSION_COLUMNS} FROM {SESSION_FROM} WHERE s.id = ?1"),
            params![id],
            read_session,
        )
        .optional()?)
}

fn read_session(row: &Row<'_>) -> rusqlite::Result<Session> {
    let role: Option<String> = row.get(6)?;
    let preview = match role {
        Some(role) => Some(read_preview(row, &role)?),
        None => None,
    };
    let updated_at: i64 = row.get(5)?;
    let last_message: Option<i64> = row.get(11)?;
    Ok(Session {
        id: row.get(0)?,
        title: row
            .get::<_, Option<String>>(1)?
            .filter(|title| !title.is_empty()),
        pinned: row.get::<_, i64>(2)? != 0,
        archived: row.get::<_, i64>(3)? != 0,
        preview,
        created_at: row.get(4)?,
        updated_at: last_message.map_or(updated_at, |last| last.max(updated_at)),
    })
}

fn read_preview(row: &Row<'_>, role: &str) -> rusqlite::Result<Preview> {
    let text: String = row.get(8)?;
    let transcript: Option<String> = row.get(9)?;
    let shown = if text.is_empty() {
        transcript.unwrap_or_default()
    } else {
        text
    };
    Ok(Preview {
        role: Role::parse(role),
        kind: Kind::parse(&row.get::<_, String>(7)?),
        text: truncate(shown.trim(), PREVIEW_CHARS),
        status: Status::parse(&row.get::<_, String>(10)?),
    })
}

fn insert_turn(
    connection: &mut Connection,
    turn: &NewTurn<'_>,
) -> Result<(Message, Message, bool), StoreError> {
    let transaction = connection.transaction()?;
    let existing: Option<String> = transaction
        .query_row(
            "SELECT id FROM chat_messages WHERE client_id = ?1",
            params![turn.client_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(user_id) = existing {
        let user = fetch(&transaction, &user_id)?.ok_or(StoreError::NotFound)?;
        let reply = fetch_reply(&transaction, &user_id)?.ok_or(StoreError::NotFound)?;
        return Ok((user, reply, false));
    }
    if fetch_session(&transaction, turn.session_id)?.is_none() {
        return Err(StoreError::SessionNotFound);
    }
    let quoted_exists = turn.reply_to.map_or(Ok(true), |quoted| {
        transaction
            .query_row(
                "SELECT seq FROM chat_messages WHERE id = ?1 AND session_id = ?2",
                params![quoted, turn.session_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map(|found| found.is_some())
    })?;
    if !quoted_exists {
        return Err(StoreError::ReplyNotFound);
    }
    let version = next_version(&transaction)?;
    transaction.execute(
        "INSERT INTO chat_messages (id, version, client_id, role, kind, text, reply_to, \
         request_id, status, error, created_at, updated_at, attachments, session_id) \
         VALUES (?1, ?2, ?3, 'user', ?7, ?4, ?5, NULL, 'done', NULL, ?6, ?6, ?8, ?9)",
        params![
            turn.user_id,
            version,
            turn.client_id,
            turn.text,
            turn.reply_to,
            turn.now,
            turn.kind.as_str(),
            encode_attachments(turn.attachments),
            turn.session_id
        ],
    )?;
    transaction.execute(
        "INSERT INTO chat_messages (id, version, client_id, role, kind, text, reply_to, \
         request_id, status, error, created_at, updated_at, session_id) \
         VALUES (?1, ?2, NULL, 'assistant', 'text', '', NULL, ?3, 'pending', NULL, ?4, ?4, ?5)",
        params![
            turn.reply_id,
            version + 1,
            turn.user_id,
            turn.now,
            turn.session_id
        ],
    )?;
    let user = fetch(&transaction, &turn.user_id)?.ok_or(StoreError::NotFound)?;
    let reply = fetch(&transaction, &turn.reply_id)?.ok_or(StoreError::NotFound)?;
    transaction.commit()?;
    Ok((user, reply, true))
}

fn update_message(
    connection: &mut Connection,
    id: &str,
    update: &Update<'_>,
) -> Result<Message, StoreError> {
    let transaction = connection.transaction()?;
    let version = next_version(&transaction)?;
    let changed = transaction.execute(
        "UPDATE chat_messages SET text = COALESCE(?2, text), status = ?3, error = ?4, \
         version = ?5, updated_at = ?6 WHERE id = ?1",
        params![
            id,
            update.text,
            update.status.as_str(),
            update.error.map(ErrorKey::as_str),
            version,
            update.now
        ],
    )?;
    if changed == 0 {
        return Err(StoreError::NotFound);
    }
    let message = fetch(&transaction, id)?.ok_or(StoreError::NotFound)?;
    transaction.commit()?;
    Ok(message)
}

fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let mut statement =
        connection.prepare("SELECT name FROM pragma_table_info('chat_messages')")?;
    let present: Vec<String> = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    for (name, kind) in ADDED_COLUMNS {
        if !present.iter().any(|column| column == name) {
            connection.execute_batch(&format!(
                "ALTER TABLE chat_messages ADD COLUMN {name} {kind}"
            ))?;
        }
    }
    Ok(())
}

fn encode_attachments(attachments: &[Attachment]) -> Option<String> {
    (!attachments.is_empty()).then(|| serde_json::to_string(attachments).unwrap_or_default())
}

fn next_version(connection: &Connection) -> Result<i64, rusqlite::Error> {
    connection.query_row(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM chat_messages",
        [],
        |row| row.get(0),
    )
}

fn fetch(connection: &Connection, id: &str) -> Result<Option<Message>, StoreError> {
    Ok(connection
        .query_row(
            &format!("SELECT {COLUMNS} FROM {FROM} WHERE m.id = ?1"),
            params![id],
            read_message,
        )
        .optional()?)
}

fn fetch_reply(connection: &Connection, user_id: &str) -> Result<Option<Message>, StoreError> {
    Ok(connection
        .query_row(
            &format!(
                "SELECT {COLUMNS} FROM {FROM} WHERE m.request_id = ?1 ORDER BY m.seq DESC LIMIT 1"
            ),
            params![user_id],
            read_message,
        )
        .optional()?)
}

fn read_message(row: &Row<'_>) -> rusqlite::Result<Message> {
    let quote_id: Option<String> = row.get(11)?;
    let quote_role: Option<String> = row.get(12)?;
    let quote_text: Option<String> = row.get(13)?;
    let reply_to = quote_id.map(|id| Quote {
        id,
        role: Role::parse(quote_role.as_deref().unwrap_or_default()),
        text: truncate(quote_text.as_deref().unwrap_or_default(), QUOTE_CHARS),
    });
    let error: Option<String> = row.get(8)?;
    let attachments: Option<String> = row.get(14)?;
    Ok(Message {
        attachments: attachments
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default(),
        transcript: row.get(15)?,
        session_id: row.get::<_, Option<String>>(17)?.unwrap_or_default(),
        alert: row
            .get::<_, Option<String>>(16)?
            .and_then(|raw| serde_json::from_str(&raw).ok()),
        seq: row.get(0)?,
        id: row.get(1)?,
        version: row.get(2)?,
        client_id: row.get(3)?,
        role: Role::parse(&row.get::<_, String>(4)?),
        kind: Kind::parse(&row.get::<_, String>(5)?),
        text: row.get(6)?,
        status: Status::parse(&row.get::<_, String>(7)?),
        error: error.as_deref().and_then(ErrorKey::parse),
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        reply_to,
    })
}

#[must_use]
pub fn truncate(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((index, _)) => format!("{}…", &text[..index]),
        None => text.to_owned(),
    }
}
