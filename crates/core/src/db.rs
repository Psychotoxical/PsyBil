//! SQLite storage: one connection, immediate transactions, schema migrations
//! via `PRAGMA user_version`.

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use rusqlite::{Connection, Row, Transaction, TransactionBehavior};
use rust_decimal::Decimal;
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::Result;
use crate::calc::PriceMode;

/// Current schema version.
pub const SCHEMA_VERSION: i32 = 1;

const MIGRATIONS: &[&str] = &[include_str!("schema_v1.sql")];

/// The PsyBil database.
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Opens (and if needed creates and migrates) the database file.
    pub fn open(path: &Path) -> Result<Db> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn)
    }

    /// An empty database in memory, for tests.
    pub fn open_in_memory() -> Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Db> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // Makes INSERT OR REPLACE fire the delete triggers, so it cannot
        // bypass the immutability rules.
        conn.pragma_update(None, "recursive_triggers", "ON")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        migrate(&mut conn)?;
        Ok(Db { conn })
    }

    /// Runs `f` in an immediate transaction. Commits on `Ok`, rolls back on
    /// `Err`.
    pub fn write<T>(&mut self, f: impl FnOnce(&Transaction) -> Result<T>) -> Result<T> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value = f(&tx)?;
        tx.commit()?;
        Ok(value)
    }

    /// The connection for reading.
    pub fn read(&self) -> &Connection {
        &self.conn
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let current: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (index, sql) in MIGRATIONS.iter().enumerate() {
        let version = index as i32 + 1;
        if version <= current {
            continue;
        }
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

fn conversion_error<E>(index: usize, error: E) -> rusqlite::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(error))
}

/// Reads a UUID stored as text.
pub(crate) fn uuid_col(row: &Row, index: usize) -> rusqlite::Result<Uuid> {
    let text: String = row.get(index)?;
    Uuid::parse_str(&text).map_err(|e| conversion_error(index, e))
}

/// Reads an optional UUID stored as text.
pub(crate) fn opt_uuid_col(row: &Row, index: usize) -> rusqlite::Result<Option<Uuid>> {
    let text: Option<String> = row.get(index)?;
    text.map(|t| Uuid::parse_str(&t).map_err(|e| conversion_error(index, e)))
        .transpose()
}

/// Reads a decimal stored as text.
pub(crate) fn dec_col(row: &Row, index: usize) -> rusqlite::Result<Decimal> {
    let text: String = row.get(index)?;
    Decimal::from_str(&text).map_err(|e| conversion_error(index, e))
}

/// Reads an optional decimal stored as text.
pub(crate) fn opt_dec_col(row: &Row, index: usize) -> rusqlite::Result<Option<Decimal>> {
    let text: Option<String> = row.get(index)?;
    text.map(|t| Decimal::from_str(&t).map_err(|e| conversion_error(index, e)))
        .transpose()
}

/// Reads a JSON value stored as text.
pub(crate) fn json_col<T: DeserializeOwned>(row: &Row, index: usize) -> rusqlite::Result<T> {
    let text: String = row.get(index)?;
    serde_json::from_str(&text).map_err(|e| conversion_error(index, e))
}

/// Reads a price mode stored as text.
pub(crate) fn price_mode_col(row: &Row, index: usize) -> rusqlite::Result<PriceMode> {
    let text: String = row.get(index)?;
    PriceMode::parse(&text).ok_or_else(|| conversion_error(index, UnknownValue(text)))
}

/// A stored text value that is not one of the expected values.
#[derive(Debug)]
pub(crate) struct UnknownValue(pub String);

impl std::fmt::Display for UnknownValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown value {:?}", self.0)
    }
}

impl std::error::Error for UnknownValue {}

/// Maps "no rows" to `Error::NotFound`.
pub(crate) fn not_found(error: rusqlite::Error) -> crate::Error {
    match error {
        rusqlite::Error::QueryReturnedNoRows => crate::Error::NotFound,
        other => crate::Error::Db(other),
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;

    #[test]
    fn insert_or_replace_cannot_bypass_triggers() {
        let db = Db::open_in_memory().unwrap();
        db.read()
            .execute(
                "INSERT INTO audit_log (id, at, entity, entity_id, action, data)
                 VALUES (1, 'a', 'e', 'x', 'create', '{}')",
                [],
            )
            .unwrap();
        let replaced = db.read().execute(
            "INSERT OR REPLACE INTO audit_log (id, at, entity, entity_id, action, data)
             VALUES (1, 'b', 'e', 'x', 'forged', '{}')",
            [],
        );
        assert!(replaced.is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    fn table_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn fresh_db_has_schema() {
        let db = Db::open_in_memory().unwrap();
        let version: i32 = db
            .read()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        let fk: i32 = db
            .read()
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        let tables = table_names(db.read());
        for t in [
            "audit_log",
            "brands",
            "businesses",
            "customers",
            "invoice_lines",
            "invoice_snapshots",
            "invoices",
            "number_counters",
            "payments",
            "products",
            "settings",
            "stock_moves",
        ] {
            assert!(tables.iter().any(|n| n == t), "missing table {t}");
        }
    }

    #[test]
    fn reopen_does_not_remigrate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.psybil");
        {
            let mut db = Db::open(&path).unwrap();
            db.write(|tx| {
                tx.execute("INSERT INTO settings (key, value) VALUES ('k', 'v')", [])?;
                Ok(())
            })
            .unwrap();
        }
        let db = Db::open(&path).unwrap();
        let version: i32 = db
            .read()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        let value: String = db
            .read()
            .query_row("SELECT value FROM settings WHERE key = 'k'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(value, "v");
    }

    #[test]
    fn write_rolls_back_on_error() {
        let mut db = Db::open_in_memory().unwrap();
        let result: Result<()> = db.write(|tx| {
            tx.execute("INSERT INTO settings (key, value) VALUES ('k', 'v')", [])?;
            Err(Error::NotFound)
        });
        assert!(result.is_err());
        let count: i64 = db
            .read()
            .query_row("SELECT COUNT(*) FROM settings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
