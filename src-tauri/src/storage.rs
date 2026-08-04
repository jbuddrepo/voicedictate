use std::{fs, path::{Path, PathBuf}};

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
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
    let serialized = serde_json::to_string_pretty(settings)?;
    fs::write(path, serialized).context("Could not save settings")
}

pub fn open_history(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
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
         );"
    )?;
    Ok(connection)
}

pub fn add_history(connection: &Connection, raw: &str, final_text: &str, duration_ms: u64, mode: &str) -> Result<()> {
    connection.execute(
        "INSERT INTO history (raw_text, final_text, duration_ms, processing_mode) VALUES (?1, ?2, ?3, ?4)",
        params![raw, final_text, duration_ms as i64, mode],
    )?;
    Ok(())
}

pub fn list_history(connection: &Connection) -> Result<Vec<HistoryEntry>> {
    let mut statement = connection.prepare(
        "SELECT id, raw_text, final_text, created_at, duration_ms, processing_mode FROM history ORDER BY id DESC"
    )?;
    let rows = statement.query_map([], |row| Ok(HistoryEntry {
        id: row.get(0)?, raw_text: row.get(1)?, final_text: row.get(2)?, created_at: row.get(3)?,
        duration_ms: row.get::<_, i64>(4)? as u64, processing_mode: row.get(5)?,
    }))?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub fn app_paths(base: PathBuf) -> (PathBuf, PathBuf, PathBuf) {
    (base.join("settings.json"), base.join("history.sqlite3"), base.join("temp"))
}
