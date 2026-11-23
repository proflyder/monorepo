use rusqlite::types::Value;
use rusqlite::{Row, params, params_from_iter};

use super::{Store, StoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    Notify,
    Find,
}

impl NotificationKind {
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Notify => "notify",
            Self::Find => "find",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "notify" => Some(Self::Notify),
            "find" => Some(Self::Find),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub id: String,
    pub device_id: String,
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
    pub deep_link: Option<String>,
    pub created_unix: i64,
    pub read_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NotificationInput {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub deep_link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationPage {
    pub entries: Vec<Notification>,
    pub next_cursor: Option<String>,
}

pub const MAX_NOTIFICATIONS_PER_DEVICE: usize = 200;
pub const DEFAULT_HISTORY_LIMIT: usize = 20;
pub const MAX_HISTORY_LIMIT: usize = 50;
pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_BODY_CHARS: usize = 500;
pub const MAX_DEEP_LINK_CHARS: usize = 512;
pub const MAX_NOTIFICATION_ID_CHARS: usize = 64;

const COLS: &str = "id, device_id, type, title, body, deep_link, created_unix, read_unix";

fn scan_notification(row: &Row<'_>) -> rusqlite::Result<Notification> {
    let kind: String = row.get(2)?;
    Ok(Notification {
        id: row.get(0)?,
        device_id: row.get(1)?,
        kind: NotificationKind::parse(&kind).unwrap_or(NotificationKind::Notify),
        title: row.get(3)?,
        body: row.get(4)?,
        deep_link: row.get(5)?,
        created_unix: row.get(6)?,
        read_unix: row.get(7)?,
    })
}

fn clean_text(value: &str, field: &str, max: usize) -> Result<String, StoreError> {
    let value = value.trim();
    if value.chars().count() > max {
        return Err(StoreError::invalid(format!("{field} is too long")));
    }
    if value.chars().any(char::is_control) {
        return Err(StoreError::invalid(format!(
            "{field} has control characters"
        )));
    }
    Ok(value.to_owned())
}

fn clean_id(raw: &str) -> Result<String, StoreError> {
    let value = raw.trim();
    if value.is_empty() || value.len() > MAX_NOTIFICATION_ID_CHARS {
        return Err(StoreError::invalid("notification id has an invalid length"));
    }
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "-_.:".contains(character))
    {
        return Err(StoreError::invalid(
            "notification id has invalid characters",
        ));
    }
    Ok(value.to_owned())
}

fn clean_cursor(raw: &str) -> Result<(i64, String), StoreError> {
    let (created, id) = raw
        .split_once(':')
        .ok_or_else(|| StoreError::invalid("cursor is invalid"))?;
    let created = created
        .parse::<i64>()
        .map_err(|_| StoreError::invalid("cursor is invalid"))?;
    Ok((created, clean_id(id)?))
}

fn cursor_of(notification: &Notification) -> String {
    format!("{}:{}", notification.created_unix, notification.id)
}

fn validate(
    input: &NotificationInput,
) -> Result<(NotificationKind, String, String, Option<String>), StoreError> {
    let kind = NotificationKind::parse(&input.kind)
        .ok_or_else(|| StoreError::invalid("type is unknown"))?;
    let title = clean_text(&input.title, "title", MAX_TITLE_CHARS)?;
    let body = clean_text(&input.body, "body", MAX_BODY_CHARS)?;
    let deep_link = input
        .deep_link
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| clean_text(value, "deep_link", MAX_DEEP_LINK_CHARS))
        .transpose()?;
    Ok((kind, title, body, deep_link))
}

impl Store {
    pub fn record_notification(
        &self,
        device_id: &str,
        input: &NotificationInput,
        now_unix: i64,
    ) -> Result<Notification, StoreError> {
        let device_id = self
            .push_device(device_id)?
            .ok_or(StoreError::NotFound)?
            .device_id;
        let (kind, title, body, deep_link) = validate(input)?;
        let id = if input.id.trim().is_empty() {
            crate::domain::id::random_id()
        } else {
            clean_id(&input.id)?
        };
        let kind_wire = kind.wire();
        let key = device_id.clone();
        let entry_id = id.clone();
        self.call(move |connection| {
            connection.execute(
                "INSERT INTO notifications (id, device_id, type, title, body, deep_link, created_unix, read_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)
                 ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title,
                    body = excluded.body,
                    deep_link = excluded.deep_link",
                params![entry_id, key, kind_wire, title, body, deep_link, now_unix],
            )?;
            connection.execute(
                "DELETE FROM notifications WHERE device_id = ?1 AND id NOT IN (
                    SELECT id FROM notifications WHERE device_id = ?1
                    ORDER BY created_unix DESC, id DESC LIMIT ?2
                )",
                params![key, i64::try_from(MAX_NOTIFICATIONS_PER_DEVICE).unwrap_or(200)],
            )?;
            Ok(())
        })?;
        self.notification(&device_id, &id)?
            .ok_or(StoreError::NotFound)
    }

    pub fn list_notifications(
        &self,
        device_id: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<NotificationPage, StoreError> {
        let limit = limit.clamp(1, MAX_HISTORY_LIMIT);
        let cursor = cursor.map(clean_cursor).transpose()?;
        let device_id = device_id.to_owned();
        let page = self.call(move |connection| {
            let limit = i64::try_from(limit).unwrap_or(20);
            let (sql, values) = match cursor {
                Some((created, id)) => (
                    format!(
                        "SELECT {COLS} FROM notifications
                         WHERE device_id = ?1 AND (created_unix < ?2 OR (created_unix = ?2 AND id < ?3))
                         ORDER BY created_unix DESC, id DESC LIMIT ?4"
                    ),
                    vec![
                        Value::Text(device_id),
                        Value::Integer(created),
                        Value::Text(id),
                        Value::Integer(limit),
                    ],
                ),
                None => (
                    format!(
                        "SELECT {COLS} FROM notifications WHERE device_id = ?1
                         ORDER BY created_unix DESC, id DESC LIMIT ?2"
                    ),
                    vec![Value::Text(device_id), Value::Integer(limit)],
                ),
            };
            let mut statement = connection.prepare(&sql)?;
            let rows = statement.query_map(params_from_iter(values), scan_notification)?;
            let mut entries = Vec::new();
            for row in rows {
                entries.push(row?);
            }
            Ok(entries)
        })?;
        let next_cursor = (page.len() == limit)
            .then(|| page.last().map(cursor_of))
            .flatten();
        Ok(NotificationPage {
            entries: page,
            next_cursor,
        })
    }

    pub fn unread_notifications(&self, device_id: &str) -> Result<i64, StoreError> {
        let device_id = device_id.to_owned();
        self.call(move |connection| {
            connection.query_row(
                "SELECT COUNT(*) FROM notifications WHERE device_id = ?1 AND read_unix = 0",
                params![device_id],
                |row| row.get(0),
            )
        })
    }

    pub fn notification(
        &self,
        device_id: &str,
        id: &str,
    ) -> Result<Option<Notification>, StoreError> {
        let device_id = device_id.to_owned();
        let id = id.to_owned();
        self.call(move |connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {COLS} FROM notifications WHERE device_id = ?1 AND id = ?2"
            ))?;
            let mut rows = statement.query(params![device_id, id])?;
            match rows.next()? {
                Some(row) => Ok(Some(scan_notification(row)?)),
                None => Ok(None),
            }
        })
    }

    pub fn set_notification_read(
        &self,
        device_id: &str,
        id: &str,
        read: bool,
        now_unix: i64,
    ) -> Result<Notification, StoreError> {
        let read_unix = if read { now_unix } else { 0 };
        let device_id = device_id.trim().to_owned();
        let id = id.trim().to_owned();
        let key = id.clone();
        let device = device_id.clone();
        let changed = self.call(move |connection| {
            connection.execute(
                "UPDATE notifications SET read_unix = ?1 WHERE device_id = ?2 AND id = ?3",
                params![read_unix, device, key],
            )
        })?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        self.notification(&device_id, &id)?
            .ok_or(StoreError::NotFound)
    }

    pub fn mark_notifications_read(
        &self,
        device_id: &str,
        now_unix: i64,
    ) -> Result<usize, StoreError> {
        let device_id = device_id.to_owned();
        self.call(move |connection| {
            connection.execute(
                "UPDATE notifications SET read_unix = ?1 WHERE device_id = ?2 AND read_unix = 0",
                params![now_unix, device_id],
            )
        })
    }

    pub fn remove_notification(&self, device_id: &str, id: &str) -> Result<bool, StoreError> {
        let device_id = device_id.to_owned();
        let id = id.to_owned();
        self.call(move |connection| {
            let deleted = connection.execute(
                "DELETE FROM notifications WHERE device_id = ?1 AND id = ?2",
                params![device_id, id],
            )?;
            Ok(deleted > 0)
        })
    }

    pub fn clear_notifications(&self, device_id: &str) -> Result<usize, StoreError> {
        let device_id = device_id.to_owned();
        self.call(move |connection| {
            connection.execute(
                "DELETE FROM notifications WHERE device_id = ?1",
                params![device_id],
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Config, PushDeviceInput};

    fn store() -> Store {
        Store::open_in_memory(Config::default()).expect("store")
    }

    fn device(store: &Store, id: &str, token: &str) {
        store
            .register_push_device(
                &PushDeviceInput {
                    device_id: id.to_owned(),
                    token: token.to_owned(),
                    platform: "android".to_owned(),
                    app_version: "0.1.0".to_owned(),
                    device_name: "Phone".to_owned(),
                },
                1,
            )
            .expect("device");
    }

    fn input(id: &str) -> NotificationInput {
        NotificationInput {
            id: id.to_owned(),
            kind: "notify".to_owned(),
            title: "Title".to_owned(),
            body: "Body".to_owned(),
            deep_link: None,
        }
    }

    fn entry(store: &Store, device_id: &str, id: &str, created: i64) -> Notification {
        store
            .record_notification(device_id, &input(id), created)
            .expect("record")
    }

    const DEVICE: &str = "device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TOKEN: &str = "token-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn a_recorded_notification_starts_unread() {
        let store = store();
        device(&store, DEVICE, TOKEN);
        let recorded = entry(&store, DEVICE, "n-1", 100);
        assert_eq!(recorded.id, "n-1");
        assert_eq!(recorded.read_unix, 0);
        assert_eq!(recorded.kind, NotificationKind::Notify);
        assert_eq!(store.unread_notifications(DEVICE).expect("unread"), 1);
    }

    #[test]
    fn an_unknown_device_is_not_stored() {
        let store = store();
        assert!(matches!(
            store.record_notification(DEVICE, &input("n-1"), 1),
            Err(StoreError::NotFound)
        ));
        assert!(
            store
                .list_notifications(DEVICE, None, 20)
                .expect("list")
                .entries
                .is_empty()
        );
    }

    #[test]
    fn the_newest_page_comes_first_and_the_cursor_continues() {
        let store = store();
        device(&store, DEVICE, TOKEN);
        for index in 0..5 {
            entry(&store, DEVICE, &format!("n-{index}"), 100 + index);
        }
        let first = store.list_notifications(DEVICE, None, 2).expect("first");
        assert_eq!(
            first
                .entries
                .iter()
                .map(|it| it.id.as_str())
                .collect::<Vec<_>>(),
            ["n-4", "n-3"]
        );
        let cursor = first.next_cursor.expect("cursor");
        let second = store
            .list_notifications(DEVICE, Some(&cursor), 2)
            .expect("second");
        assert_eq!(
            second
                .entries
                .iter()
                .map(|it| it.id.as_str())
                .collect::<Vec<_>>(),
            ["n-2", "n-1"]
        );
        let third = store
            .list_notifications(DEVICE, second.next_cursor.as_deref(), 2)
            .expect("third");
        assert_eq!(
            third
                .entries
                .iter()
                .map(|it| it.id.as_str())
                .collect::<Vec<_>>(),
            ["n-0"]
        );
        assert!(third.next_cursor.is_none());
    }

    #[test]
    fn history_is_capped_per_device_and_keeps_other_devices() {
        let store = store();
        device(&store, DEVICE, TOKEN);
        device(
            &store,
            "device-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "token-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
        entry(
            &store,
            "device-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "other",
            1,
        );
        for index in 0..(MAX_NOTIFICATIONS_PER_DEVICE + 5) {
            entry(
                &store,
                DEVICE,
                &format!("n-{index:04}"),
                100 + i64::try_from(index).unwrap(),
            );
        }
        let page = store
            .list_notifications(DEVICE, None, MAX_HISTORY_LIMIT)
            .expect("page");
        assert_eq!(page.entries.len(), MAX_HISTORY_LIMIT);
        assert_eq!(store.unread_notifications(DEVICE).expect("unread"), 200);
        assert_eq!(
            store
                .list_notifications("device-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", None, 20)
                .expect("other")
                .entries
                .len(),
            1
        );
    }

    #[test]
    fn read_state_toggles_and_read_all_only_touches_unread_rows() {
        let store = store();
        device(&store, DEVICE, TOKEN);
        entry(&store, DEVICE, "n-1", 100);
        entry(&store, DEVICE, "n-2", 200);
        let read = store
            .set_notification_read(DEVICE, "n-1", true, 300)
            .expect("read");
        assert_eq!(read.read_unix, 300);
        assert_eq!(store.unread_notifications(DEVICE).expect("unread"), 1);
        let unread = store
            .set_notification_read(DEVICE, "n-1", false, 400)
            .expect("unread");
        assert_eq!(unread.read_unix, 0);
        assert_eq!(store.mark_notifications_read(DEVICE, 500).expect("all"), 2);
        assert_eq!(store.unread_notifications(DEVICE).expect("unread"), 0);
        assert!(matches!(
            store.set_notification_read(DEVICE, "missing", true, 1),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn removal_is_scoped_to_the_device() {
        let store = store();
        device(&store, DEVICE, TOKEN);
        device(
            &store,
            "device-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "token-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
        entry(&store, DEVICE, "n-1", 100);
        assert!(
            !store
                .remove_notification("device-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "n-1")
                .expect("remove")
        );
        assert!(store.remove_notification(DEVICE, "n-1").expect("remove"));
        assert!(
            store
                .list_notifications(DEVICE, None, 20)
                .expect("list")
                .entries
                .is_empty()
        );
    }

    #[test]
    fn clearing_removes_every_entry_of_the_device() {
        let store = store();
        device(&store, DEVICE, TOKEN);
        entry(&store, DEVICE, "n-1", 100);
        entry(&store, DEVICE, "n-2", 200);
        assert_eq!(store.clear_notifications(DEVICE).expect("clear"), 2);
        assert_eq!(store.clear_notifications(DEVICE).expect("clear again"), 0);
    }

    #[test]
    fn removing_the_device_removes_its_history() {
        let store = store();
        device(&store, DEVICE, TOKEN);
        entry(&store, DEVICE, "n-1", 100);
        assert!(store.remove_push_device(TOKEN).expect("remove"));
        assert!(
            store
                .list_notifications(DEVICE, None, 20)
                .expect("list")
                .entries
                .is_empty()
        );
    }

    #[test]
    fn an_invalid_type_or_oversized_text_is_rejected() {
        let store = store();
        device(&store, DEVICE, TOKEN);
        let unknown = NotificationInput {
            kind: "sync".to_owned(),
            ..input("n-1")
        };
        assert!(store.record_notification(DEVICE, &unknown, 1).is_err());
        let long = NotificationInput {
            title: "x".repeat(MAX_TITLE_CHARS + 1),
            ..input("n-1")
        };
        assert!(store.record_notification(DEVICE, &long, 1).is_err());
        let invalid = NotificationInput {
            id: "bad id".to_owned(),
            ..input("n-1")
        };
        assert!(store.record_notification(DEVICE, &invalid, 1).is_err());
    }

    #[test]
    fn an_explicit_id_makes_recording_idempotent() {
        let store = store();
        device(&store, DEVICE, TOKEN);
        entry(&store, DEVICE, "n-1", 100);
        let again = store
            .record_notification(DEVICE, &input("n-1"), 200)
            .expect("again");
        assert_eq!(again.id, "n-1");
        assert_eq!(
            store
                .list_notifications(DEVICE, None, 20)
                .expect("list")
                .entries
                .len(),
            1
        );
    }
}
