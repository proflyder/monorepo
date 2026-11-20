use std::path::Path;
use std::sync::{Mutex, PoisonError};

use rusqlite::{Connection, OptionalExtension as _, Row, params};

use super::model::{Attachment, ErrorKey, Kind, Message, Quote, Role, Status};

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
    transcript TEXT
);
CREATE INDEX IF NOT EXISTS chat_messages_version ON chat_messages(version);
CREATE INDEX IF NOT EXISTS chat_messages_request ON chat_messages(request_id);
";

const COLUMNS: &str = "m.seq, m.id, m.version, m.client_id, m.role, m.kind, m.text, m.status, \
     m.error, m.created_at, m.updated_at, q.id, q.role, q.text, m.attachments, m.transcript";

const ADDED_COLUMNS: [(&str, &str); 2] = [("attachments", "TEXT"), ("transcript", "TEXT")];

const FROM: &str = "chat_messages m LEFT JOIN chat_messages q ON q.id = m.reply_to";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("chat storage: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("chat storage: quoted message not found")]
    ReplyNotFound,
    #[error("chat storage: message not found")]
    NotFound,
}

pub struct NewTurn<'a> {
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
        before: Option<i64>,
        limit: usize,
    ) -> Result<(Vec<Message>, bool), StoreError> {
        let bound = before.unwrap_or(i64::MAX);
        let fetch_limit = i64::try_from(limit + 1).unwrap_or(i64::MAX);
        let mut rows = self.with(|connection| {
            query(
                connection,
                &format!(
                    "SELECT {COLUMNS} FROM {FROM} WHERE m.seq < ?1 ORDER BY m.seq DESC LIMIT ?2"
                ),
                bound,
                fetch_limit,
            )
        })?;
        let has_more = rows.len() > limit;
        rows.truncate(limit);
        rows.reverse();
        Ok((rows, has_more))
    }

    pub fn changed_after(&self, version: i64, limit: usize) -> Result<Vec<Message>, StoreError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with(|connection| {
            query(
                connection,
                &format!(
                    "SELECT {COLUMNS} FROM {FROM} WHERE m.version > ?1 ORDER BY m.version ASC LIMIT ?2"
                ),
                version,
                limit,
            )
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

fn query(
    connection: &Connection,
    sql: &str,
    bound: i64,
    limit: i64,
) -> Result<Vec<Message>, StoreError> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement
        .query_map(params![bound, limit], read_message)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
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
    let quoted_exists = turn.reply_to.map_or(Ok(true), |quoted| {
        transaction
            .query_row(
                "SELECT seq FROM chat_messages WHERE id = ?1",
                params![quoted],
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
         request_id, status, error, created_at, updated_at, attachments) \
         VALUES (?1, ?2, ?3, 'user', ?7, ?4, ?5, NULL, 'done', NULL, ?6, ?6, ?8)",
        params![
            turn.user_id,
            version,
            turn.client_id,
            turn.text,
            turn.reply_to,
            turn.now,
            turn.kind.as_str(),
            encode_attachments(turn.attachments)
        ],
    )?;
    transaction.execute(
        "INSERT INTO chat_messages (id, version, client_id, role, kind, text, reply_to, \
         request_id, status, error, created_at, updated_at) \
         VALUES (?1, ?2, NULL, 'assistant', 'text', '', NULL, ?3, 'pending', NULL, ?4, ?4)",
        params![turn.reply_id, version + 1, turn.user_id, turn.now],
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
