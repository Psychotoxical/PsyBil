//! SQLite storage: one connection, immediate transactions, schema migrations
//! via `PRAGMA user_version`.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, Row, Transaction, TransactionBehavior};
use rust_decimal::Decimal;
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::calc::PriceMode;
use crate::{Error, Result};

/// Current schema version.
pub const SCHEMA_VERSION: i32 = 2;

/// One schema step: SQL, then an optional data step in the same transaction.
struct Migration {
    sql: &'static str,
    rust: Option<fn(&Transaction) -> Result<()>>,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        sql: include_str!("schema_v1.sql"),
        rust: None,
    },
    Migration {
        sql: include_str!("schema_v2.sql"),
        rust: Some(crate::masterdata::rebase_stock_v2),
    },
];

/// The PsyBil database.
pub struct Db {
    conn: Connection,
    path: Option<PathBuf>,
}

impl Db {
    /// Opens (and if needed creates and migrates) the database file. A file
    /// from a newer PsyBil version is refused without being touched.
    pub fn open(path: &Path) -> Result<Db> {
        let conn = Connection::open(path)?;
        let found = user_version(&conn)?;
        if found > SCHEMA_VERSION {
            return Err(Error::NewerSchema {
                found,
                supported: SCHEMA_VERSION,
            });
        }
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn, Some(path.to_path_buf()))
    }

    /// Opens a database file for reading only, without migrating it.
    pub fn open_read_only(path: &Path) -> Result<Db> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let found = user_version(&conn)?;
        if found > SCHEMA_VERSION {
            return Err(Error::NewerSchema {
                found,
                supported: SCHEMA_VERSION,
            });
        }
        if found < SCHEMA_VERSION {
            return Err(Error::OlderSchema { found });
        }
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(Db {
            conn,
            path: Some(path.to_path_buf()),
        })
    }

    /// An empty database in memory, for tests.
    pub fn open_in_memory() -> Result<Db> {
        Self::init(Connection::open_in_memory()?, None)
    }

    fn init(mut conn: Connection, path: Option<PathBuf>) -> Result<Db> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // Makes INSERT OR REPLACE fire the delete triggers, so it cannot
        // bypass the immutability rules.
        conn.pragma_update(None, "recursive_triggers", "ON")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        migrate(&mut conn)?;
        Ok(Db { conn, path })
    }

    /// The file this database was opened from.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Attaches another database file under an internal alias.
    pub(crate) fn attach(&self, path: &Path, alias: &str) -> Result<()> {
        self.conn.execute(
            &format!("ATTACH DATABASE ?1 AS {alias}"),
            [path.to_string_lossy().to_string()],
        )?;
        Ok(())
    }

    pub(crate) fn detach(&self, alias: &str) -> Result<()> {
        self.conn.execute(&format!("DETACH DATABASE {alias}"), [])?;
        Ok(())
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

fn user_version(conn: &Connection) -> Result<i32> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let current = user_version(conn)?;
    for (index, migration) in MIGRATIONS.iter().enumerate() {
        let version = index as i32 + 1;
        if version <= current {
            continue;
        }
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(migration.sql)?;
        if let Some(step) = migration.rust {
            step(&tx)?;
        }
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
mod v2_tests {
    use super::*;
    use crate::Error;
    use crate::issue::sha256_hex;
    use crate::testutil::clock;

    fn v1_file(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("old.psybil");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("schema_v1.sql")).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO audit_log (at, entity, entity_id, action, data)
             VALUES ('2026-01-01T00:00:00.000Z', 'customer', 'x', 'create', '{}')",
            [],
        )
        .unwrap();
        path
    }

    #[test]
    fn v1_database_migrates_to_v2() {
        let dir = tempfile::tempdir().unwrap();
        let path = v1_file(dir.path());
        let db = Db::open(&path).unwrap();
        let version: i32 = db
            .read()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 2);
        let uid: Option<String> = db
            .read()
            .query_row("SELECT uid FROM audit_log", [], |r| r.get(0))
            .unwrap();
        assert!(uid.is_some_and(|u| u.len() >= 32));
        let tables: i64 = db
            .read()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name IN ('tombstones', 'merge_quarantine')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 2);
    }

    #[test]
    fn newer_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.psybil");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE x (a); PRAGMA user_version = 3;")
                .unwrap();
        }
        let before = sha256_hex(&std::fs::read(&path).unwrap());
        assert!(matches!(
            Db::open(&path),
            Err(Error::NewerSchema {
                found: 3,
                supported: 2
            })
        ));
        assert!(matches!(
            Db::open_read_only(&path),
            Err(Error::NewerSchema { found: 3, .. })
        ));
        assert_eq!(sha256_hex(&std::fs::read(&path).unwrap()), before);
    }

    #[test]
    fn read_only_cannot_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ro.psybil");
        drop(Db::open(&path).unwrap());
        let mut db = Db::open_read_only(&path).unwrap();
        assert_eq!(db.path(), Some(path.as_path()));
        let result = db.write(|tx| {
            tx.execute("INSERT INTO settings (key, value) VALUES ('k', 'v')", [])?;
            Ok(())
        });
        assert!(result.is_err());
    }

    #[test]
    fn audit_rows_get_uid() {
        let mut db = Db::open_in_memory().unwrap();
        db.write(|tx| {
            crate::audit::log(
                tx,
                &clock(),
                "customer",
                Uuid::new_v4(),
                "update",
                serde_json::json!({}),
            )
        })
        .unwrap();
        let uid: Option<String> = db
            .read()
            .query_row("SELECT uid FROM audit_log", [], |r| r.get(0))
            .unwrap();
        assert!(uid.is_some());
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
