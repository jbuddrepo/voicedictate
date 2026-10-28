use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::models::{AppSettings, HistoryEntry};

pub fn load_settings(path: &Path) -> AppSettings {
    fs::read_to_string(path)
        .ok()
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

pub fn save_settings(path: &Path, settings: &AppSettings) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let serialized = serde_json::to_string_pretty(settings)?;
    fs::write(path, serialized).context("Could not save settings")
}

pub fn open_history(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let connection = Connection::open(path)?;
    connection.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS history (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           raw_text TEXT NOT NULL,
           final_text TEXT NOT NULL,
           created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
           duration_ms INTEGER NOT NULL,
           processing_mode TEXT NOT NULL
         );",
    )?;
    let columns: Vec<String> = connection
        .prepare("PRAGMA table_info(history)")?
        .query_map([], |row| row.get(1))?
        .collect::<Result<_, _>>()?;
    if !columns.iter().any(|name| name == "transcription_source") {
        connection.execute(
            "ALTER TABLE history ADD COLUMN transcription_source TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    if !columns.iter().any(|name| name == "processing_ms") {
        connection.execute(
            "ALTER TABLE history ADD COLUMN processing_ms INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    Ok(connection)
}

pub fn add_history(
    connection: &Connection,
    raw: &str,
    final_text: &str,
    duration_ms: u64,
    mode: &str,
    source: &str,
    processing_ms: u64,
) -> Result<()> {
    connection.execute(
        "INSERT INTO history (raw_text, final_text, duration_ms, processing_mode, transcription_source, processing_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![raw, final_text, duration_ms as i64, mode, source, processing_ms as i64],
    )?;
    Ok(())
}

pub fn list_history(connection: &Connection) -> Result<Vec<HistoryEntry>> {
    let mut statement = connection.prepare(
        "SELECT id, raw_text, final_text, created_at, duration_ms, processing_mode, transcription_source, processing_ms FROM history ORDER BY id DESC"
    )?;
    let rows = statement.query_map([], |row| {
        Ok(HistoryEntry {
            id: row.get(0)?,
            raw_text: row.get(1)?,
            final_text: row.get(2)?,
            created_at: row.get(3)?,
            duration_ms: row.get::<_, i64>(4)? as u64,
            processing_mode: row.get(5)?,
            transcription_source: row.get(6)?,
            processing_ms: row.get::<_, i64>(7)?.max(0) as u64,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_existing_history_without_losing_text_and_keeps_timings_separate() {
        let path = std::env::temp_dir().join(format!(
            "voicedictate-history-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        {
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch("CREATE TABLE history (id INTEGER PRIMARY KEY AUTOINCREMENT, raw_text TEXT NOT NULL, final_text TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT '', duration_ms INTEGER NOT NULL, processing_mode TEXT NOT NULL); INSERT INTO history(raw_text,final_text,duration_ms,processing_mode) VALUES ('old text','old text',5000,'raw');").unwrap();
        }
        {
            let connection = open_history(&path).unwrap();
            add_history(
                &connection,
                "new text",
                "new text",
                8000,
                "raw",
                "local:tiny",
                400,
            )
            .unwrap();
            let entries = list_history(&connection).unwrap();
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].processing_ms, 400);
            assert_eq!(entries[0].duration_ms, 8000);
            assert_eq!(entries[0].transcription_source, "local:tiny");
            assert_eq!(entries[1].raw_text, "old text");
            assert_eq!(entries[1].processing_ms, 0);
            assert!(entries[1].transcription_source.is_empty());
        }
        // Opening again is idempotent after the migration.
        drop(open_history(&path).unwrap());
        fs::remove_file(path).unwrap();
    }
}

pub fn app_paths(base: PathBuf) -> (PathBuf, PathBuf, PathBuf) {
    (
        base.join("settings.json"),
        base.join("history.sqlite3"),
        base.join("temp"),
    )
}
