//! Merging two copies of a PsyBil database, for example after the cloud sync
//! brought in a state another computer wrote.
//!
//! Master data and drafts: the newer change wins; with the common base copy
//! a change on both sides is reported. Issued documents, payments, stock
//! bookings and the audit log are only ever added, never changed. Deletion
//! marks keep deleted rows from coming back.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, Transaction, params};
use serde::Serialize;
use uuid::Uuid;

use crate::audit;
use crate::dates::Clock;
use crate::db::{Db, uuid_col};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Side {
    Local,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangedBoth {
    pub table: String,
    pub id: Uuid,
    pub kept: Side,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MergeReport {
    pub added: usize,
    pub updated: usize,
    pub deleted: usize,
    /// Rows changed on both sides since the base; the newer one was kept.
    pub changed_both: Vec<ChangedBoth>,
    /// Rows that differ while no base was available; the newer one was kept.
    pub unclear: Vec<ChangedBoth>,
    /// Quarantine ids of documents that could not be placed.
    pub quarantined: Vec<String>,
    /// Deletions from the other side that were not applied here.
    pub skipped_deletes: Vec<(String, Uuid)>,
    /// Rows deleted here that came back because the other side's documents
    /// refer to them.
    pub restored: Vec<(String, Uuid)>,
}

const MASTER_TABLES: [&str; 4] = ["businesses", "brands", "customers", "products"];

/// A private folder for copies of the databases being merged.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new() -> Result<Scratch> {
        let dir = std::env::temp_dir().join(format!("psybil-merge-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).map_err(io_error)?;
        Ok(Scratch { dir })
    }

    /// Copies a database and brings the copy to the current schema.
    fn prepare(&self, source: &Path, name: &str) -> Result<PathBuf> {
        let target = self.dir.join(name);
        std::fs::copy(source, &target).map_err(io_error)?;
        drop(Db::open(&target)?);
        Ok(target)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub(crate) fn io_error(error: std::io::Error) -> Error {
    Error::Io(error.to_string())
}

impl Db {
    /// Merges the database file `other` into this one. `base` is the copy
    /// both sides started from, when known.
    pub fn merge_from(
        &mut self,
        clock: &dyn Clock,
        other: &Path,
        base: Option<&Path>,
    ) -> Result<MergeReport> {
        let scratch = Scratch::new()?;
        let other_copy = scratch.prepare(other, "other.psybil")?;
        let base_copy = base
            .map(|b| scratch.prepare(b, "base.psybil"))
            .transpose()?;
        self.attach(&other_copy, "other")?;
        if let Some(b) = &base_copy {
            self.attach(b, "base")?;
        }
        let has_base = base_copy.is_some();
        let result = self.write(|tx| merge_all(tx, clock, has_base));
        self.detach("other")?;
        if has_base {
            self.detach("base")?;
        }
        result
    }
}

fn merge_all(tx: &Transaction, clock: &dyn Clock, has_base: bool) -> Result<MergeReport> {
    tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;
    let mut report = MergeReport::default();
    for table in MASTER_TABLES {
        merge_rows(tx, &mut report, table, has_base)?;
    }
    let mut quarantined = merge_issued(tx, clock, &mut report)?;
    merge_drafts(tx, clock, &mut report, has_base, &mut quarantined)?;
    merge_payments(tx, &mut report, &quarantined)?;
    append_only(tx, &mut report, &quarantined)?;
    apply_tombstones(tx, clock, &mut report, has_base)?;
    restore_parents(tx, &mut report)?;
    audit::log(
        tx,
        clock,
        "database",
        Uuid::nil(),
        "merge",
        serde_json::to_value(&report)?,
    )?;
    Ok(report)
}

fn columns(tx: &Transaction, table: &str) -> Result<Vec<String>> {
    let mut stmt = tx.prepare(&format!("PRAGMA main.table_info({table})"))?;
    let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
    Ok(names.collect::<rusqlite::Result<_>>()?)
}

fn updated_at(tx: &Transaction, schema: &str, table: &str, id: Uuid) -> Result<Option<String>> {
    Ok(tx
        .query_row(
            &format!("SELECT updated_at FROM {schema}.{table} WHERE id = ?1"),
            [id.to_string()],
            |r| r.get(0),
        )
        .optional()?)
}

fn tombstoned(tx: &Transaction, table: &str, id: Uuid) -> Result<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM main.tombstones WHERE table_name = ?1 AND id = ?2)",
        params![table, id.to_string()],
        |r| r.get(0),
    )?)
}

fn copy_row(tx: &Transaction, table: &str, cols: &[String], id: Uuid) -> Result<()> {
    let list = cols.join(", ");
    tx.execute(
        &format!(
            "INSERT INTO main.{table} ({list}) SELECT {list} FROM other.{table} WHERE id = ?1"
        ),
        [id.to_string()],
    )?;
    Ok(())
}

fn update_row(tx: &Transaction, table: &str, cols: &[String], id: Uuid) -> Result<()> {
    let list = cols
        .iter()
        .filter(|c| c.as_str() != "id")
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    tx.execute(
        &format!(
            "UPDATE main.{table} SET ({list}) = (SELECT {list} FROM other.{table} WHERE id = ?1)
             WHERE id = ?1"
        ),
        [id.to_string()],
    )?;
    Ok(())
}

/// Which side wins when both have the row with different `updated_at`.
fn decide(
    tx: &Transaction,
    report: &mut MergeReport,
    table: &str,
    id: Uuid,
    local_at: &str,
    other_at: &str,
    has_base: bool,
) -> Result<Side> {
    let newer = if other_at > local_at {
        Side::Other
    } else {
        Side::Local
    };
    if !has_base {
        report.unclear.push(ChangedBoth {
            table: table.into(),
            id,
            kept: newer,
        });
        return Ok(newer);
    }
    let base_at = updated_at(tx, "base", table, id)?;
    let local_changed = base_at.as_deref() != Some(local_at);
    let other_changed = base_at.as_deref() != Some(other_at);
    Ok(match (local_changed, other_changed) {
        (false, _) => Side::Other,
        (true, false) => Side::Local,
        (true, true) => {
            report.changed_both.push(ChangedBoth {
                table: table.into(),
                id,
                kept: newer,
            });
            newer
        }
    })
}

/// Last-writer-wins for one master data table.
fn merge_rows(
    tx: &Transaction,
    report: &mut MergeReport,
    table: &str,
    has_base: bool,
) -> Result<()> {
    let cols = columns(tx, table)?;
    let rows: Vec<(Uuid, String)> = {
        let mut stmt = tx.prepare(&format!("SELECT id, updated_at FROM other.{table}"))?;
        stmt.query_map([], |r| Ok((uuid_col(r, 0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    for (id, other_at) in rows {
        if tombstoned(tx, table, id)? {
            continue;
        }
        match updated_at(tx, "main", table, id)? {
            None => {
                copy_row(tx, table, &cols, id)?;
                report.added += 1;
            }
            Some(local_at) if local_at == other_at => {}
            Some(local_at) => {
                if decide(tx, report, table, id, &local_at, &other_at, has_base)? == Side::Other {
                    update_row(tx, table, &cols, id)?;
                    report.updated += 1;
                }
            }
        }
    }
    Ok(())
}

fn replace_lines(tx: &Transaction, invoice_id: Uuid) -> Result<()> {
    let cols = columns(tx, "invoice_lines")?.join(", ");
    tx.execute(
        "DELETE FROM main.invoice_lines WHERE invoice_id = ?1",
        [invoice_id.to_string()],
    )?;
    tx.execute(
        &format!(
            "INSERT INTO main.invoice_lines ({cols}) SELECT {cols} FROM other.invoice_lines
             WHERE invoice_id = ?1"
        ),
        [invoice_id.to_string()],
    )?;
    Ok(())
}

fn local_state(tx: &Transaction, id: Uuid) -> Result<Option<String>> {
    Ok(tx
        .query_row(
            "SELECT state FROM main.invoices WHERE id = ?1",
            [id.to_string()],
            |r| r.get(0),
        )
        .optional()?)
}

/// Drafts follow the master data rules; a draft that is issued on this side
/// stays issued. A replacement draft for a quarantined invoice goes into
/// the quarantine as well.
fn merge_drafts(
    tx: &Transaction,
    clock: &dyn Clock,
    report: &mut MergeReport,
    has_base: bool,
    quarantined: &mut HashSet<Uuid>,
) -> Result<()> {
    let cols = columns(tx, "invoices")?;
    let rows: Vec<(Uuid, String, Option<String>)> = {
        let mut stmt = tx.prepare(
            "SELECT id, updated_at, replaces_id FROM other.invoices WHERE state = 'draft'",
        )?;
        stmt.query_map([], |r| Ok((uuid_col(r, 0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    for (id, other_at, replaces) in rows {
        if tombstoned(tx, "invoices", id)? {
            continue;
        }
        let state = local_state(tx, id)?;
        if state.is_none() && refers_to(&replaces, quarantined) {
            report.quarantined.push(quarantine_document(
                tx,
                clock,
                id,
                "depends_on_quarantined",
            )?);
            quarantined.insert(id);
            continue;
        }
        match state.as_deref() {
            None => {
                copy_row(tx, "invoices", &cols, id)?;
                replace_lines(tx, id)?;
                report.added += 1;
            }
            Some("issued") => {}
            Some(_) => {
                let local_at = updated_at(tx, "main", "invoices", id)?.unwrap_or_default();
                if local_at != other_at
                    && decide(tx, report, "invoices", id, &local_at, &other_at, has_base)?
                        == Side::Other
                {
                    replace_lines(tx, id)?;
                    update_row(tx, "invoices", &cols, id)?;
                    report.updated += 1;
                }
            }
        }
    }
    Ok(())
}

/// Rows of a query as JSON objects, for the quarantine.
fn rows_json(tx: &Transaction, sql: &str, id: Uuid) -> Result<serde_json::Value> {
    use rusqlite::types::ValueRef;
    let mut stmt = tx.prepare(sql)?;
    let names: Vec<String> = stmt.column_names().map(|n| n.to_string()).collect();
    let mut rows = stmt.query([id.to_string()])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut object = serde_json::Map::new();
        for (index, name) in names.iter().enumerate() {
            let value = match row.get_ref(index)? {
                ValueRef::Null => serde_json::Value::Null,
                ValueRef::Integer(i) => serde_json::Value::from(i),
                ValueRef::Real(f) => serde_json::Value::from(f),
                ValueRef::Text(t) => {
                    serde_json::Value::from(String::from_utf8_lossy(t).to_string())
                }
                ValueRef::Blob(b) => {
                    use base64::Engine;
                    serde_json::Value::from(base64::engine::general_purpose::STANDARD.encode(b))
                }
            };
            object.insert(name.clone(), value);
        }
        out.push(serde_json::Value::Object(object));
    }
    Ok(serde_json::Value::Array(out))
}

/// Puts a document of the other side aside instead of merging it.
pub(crate) fn quarantine(
    tx: &Transaction,
    clock: &dyn Clock,
    kind: &str,
    reason: &str,
    payload: serde_json::Value,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO main.merge_quarantine (id, kind, reason, payload, at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, kind, reason, payload.to_string(), crate::dates::timestamp(clock)],
    )?;
    Ok(id)
}

/// Puts the other side's document `id` with its lines, snapshot and
/// payments into the quarantine.
fn quarantine_document(
    tx: &Transaction,
    clock: &dyn Clock,
    id: Uuid,
    reason: &str,
) -> Result<String> {
    let payload = serde_json::json!({
        "invoice": rows_json(tx, "SELECT * FROM other.invoices WHERE id = ?1", id)?,
        "lines": rows_json(tx, "SELECT * FROM other.invoice_lines WHERE invoice_id = ?1", id)?,
        "snapshot": rows_json(tx, "SELECT * FROM other.invoice_snapshots WHERE invoice_id = ?1", id)?,
        "payments": rows_json(tx, "SELECT * FROM other.payments WHERE invoice_id = ?1", id)?,
    });
    quarantine(tx, clock, "invoice", reason, payload)
}

fn refers_to(target: &Option<String>, set: &HashSet<Uuid>) -> bool {
    target
        .as_deref()
        .and_then(|t| Uuid::parse_str(t).ok())
        .is_some_and(|t| set.contains(&t))
}

/// Whether both sides hold the same issued document `id`: same number,
/// same totals, same lines.
fn same_document(tx: &Transaction, id: Uuid) -> Result<bool> {
    let fingerprint = |schema: &str| -> Result<Option<String>> {
        Ok(tx
            .query_row(
                &format!(
                    "SELECT i.business_id || '|' || ifnull(i.number, '') || '|' ||
                            ifnull(i.net_cents, '') || '|' || ifnull(i.tax_cents, '') || '|' ||
                            ifnull(i.gross_cents, '') || '|' ||
                            ifnull((SELECT group_concat(l.kind || ':' || l.description || ':' ||
                                        l.quantity || ':' || l.unit_price || ':' || l.tax, ';')
                                    FROM (SELECT * FROM {schema}.invoice_lines
                                          WHERE invoice_id = i.id ORDER BY position) l), '')
                     FROM {schema}.invoices i WHERE i.id = ?1"
                ),
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()?)
    };
    Ok(fingerprint("main")? == fingerprint("other")?)
}

/// Issued invoices of the other side are only ever added. Returns the ids
/// of the other side's documents that went into the quarantine.
fn merge_issued(
    tx: &Transaction,
    clock: &dyn Clock,
    report: &mut MergeReport,
) -> Result<HashSet<Uuid>> {
    let cols = columns(tx, "invoices")?;
    let list = cols.join(", ");
    let as_draft = cols
        .iter()
        .map(|c| match c.as_str() {
            "state" => "'draft'".to_string(),
            "cancelled_by_id" => "NULL".to_string(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    type IssuedRow = (Uuid, String, String, Option<String>, Option<String>);
    let rows: Vec<IssuedRow> = {
        let mut stmt = tx.prepare(
            "SELECT id, business_id, number, cancels_id, replaces_id FROM other.invoices
             WHERE state = 'issued' ORDER BY created_at",
        )?;
        stmt.query_map([], |r| {
            Ok((uuid_col(r, 0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<_>>()?
    };
    let mut quarantined = HashSet::new();
    for (id, business_id, number, cancels, replaces) in rows {
        let state = local_state(tx, id)?;
        let reason = if refers_to(&cancels, &quarantined) || refers_to(&replaces, &quarantined) {
            Some("depends_on_quarantined")
        } else if state.as_deref() == Some("issued") {
            if same_document(tx, id)? {
                continue;
            }
            Some("issued_on_both_sides")
        } else if cancels.is_some()
            && tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM main.invoices WHERE cancels_id = ?1 AND id != ?2)",
                params![cancels, id.to_string()],
                |r| r.get::<_, bool>(0),
            )?
        {
            Some("cancelled_on_both_sides")
        } else if tx
            .query_row(
                "SELECT id FROM main.invoices WHERE business_id = ?1 AND number = ?2 AND id != ?3",
                params![business_id, number, id.to_string()],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .is_some()
        {
            Some("number_collision")
        } else {
            None
        };
        if let Some(reason) = reason {
            report
                .quarantined
                .push(quarantine_document(tx, clock, id, reason)?);
            quarantined.insert(id);
            continue;
        }
        if state.is_none() {
            tx.execute(
                &format!(
                    "INSERT INTO main.invoices ({list}) SELECT {as_draft} FROM other.invoices
                     WHERE id = ?1"
                ),
                [id.to_string()],
            )?;
            report.added += 1;
        } else {
            report.updated += 1;
        }
        // Lines first, while the invoice is still a draft here.
        replace_lines(tx, id)?;
        tx.execute(
            &format!(
                "UPDATE main.invoices SET ({list}) = (SELECT {as_draft} FROM other.invoices
                 WHERE id = ?1) WHERE id = ?1"
            ),
            [id.to_string()],
        )?;
        tx.execute(
            "UPDATE main.invoices SET state = 'issued' WHERE id = ?1",
            [id.to_string()],
        )?;
    }
    tx.execute(
        "INSERT INTO main.invoice_snapshots SELECT * FROM other.invoice_snapshots o
         WHERE o.invoice_id NOT IN (SELECT invoice_id FROM main.invoice_snapshots)
           AND o.invoice_id IN (SELECT id FROM main.invoices WHERE state = 'issued')",
        [],
    )?;
    // Cancellation links, once both documents are here.
    tx.execute(
        "UPDATE main.invoices SET cancelled_by_id =
           (SELECT o.cancelled_by_id FROM other.invoices o WHERE o.id = invoices.id)
         WHERE cancelled_by_id IS NULL AND state = 'issued'
           AND EXISTS (SELECT 1 FROM other.invoices o WHERE o.id = invoices.id
                       AND o.cancelled_by_id IN (SELECT id FROM main.invoices))",
        [],
    )?;
    Ok(quarantined)
}

/// Payments are only added; a move to a replacement invoice is taken over.
fn merge_payments(
    tx: &Transaction,
    report: &mut MergeReport,
    quarantined: &std::collections::HashSet<Uuid>,
) -> Result<()> {
    let cols = columns(tx, "payments")?;
    let rows: Vec<(Uuid, Uuid)> = {
        let mut stmt =
            tx.prepare("SELECT id, invoice_id FROM other.payments ORDER BY created_at")?;
        stmt.query_map([], |r| Ok((uuid_col(r, 0)?, uuid_col(r, 1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    for (id, invoice_id) in rows {
        if quarantined.contains(&invoice_id) || tombstoned(tx, "payments", id)? {
            continue;
        }
        if local_state(tx, invoice_id)?.is_none() {
            continue;
        }
        if updated_at(tx, "main", "payments", id)?.is_some() {
            tx.execute(
                "UPDATE main.payments SET transferred_to_id =
                   (SELECT o.transferred_to_id FROM other.payments o WHERE o.id = ?1)
                 WHERE id = ?1 AND transferred_to_id IS NULL
                   AND (SELECT o.transferred_to_id FROM other.payments o WHERE o.id = ?1)
                       IN (SELECT id FROM main.invoices)",
                [id.to_string()],
            )?;
            continue;
        }
        copy_row(tx, "payments", &cols, id)?;
        tx.execute(
            "UPDATE main.payments SET transferred_to_id = NULL
             WHERE id = ?1 AND transferred_to_id NOT IN (SELECT id FROM main.invoices)",
            [id.to_string()],
        )?;
        report.added += 1;
    }
    Ok(())
}

/// Stock bookings, audit rows, quarantine rows and counters: add what is
/// missing. Bookings of quarantined documents stay out.
fn append_only(
    tx: &Transaction,
    report: &mut MergeReport,
    quarantined: &HashSet<Uuid>,
) -> Result<()> {
    let moves: Vec<(String, Uuid)> = {
        let mut stmt = tx.prepare(
            "SELECT o.id, o.invoice_id FROM other.stock_moves o
             WHERE o.id NOT IN (SELECT id FROM main.stock_moves)
               AND o.invoice_id IN (SELECT id FROM main.invoices)
               AND o.product_id IN (SELECT id FROM main.products)",
        )?;
        stmt.query_map([], |r| Ok((r.get(0)?, uuid_col(r, 1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    for (id, invoice_id) in moves {
        if quarantined.contains(&invoice_id) {
            continue;
        }
        report.added += tx.execute(
            "INSERT INTO main.stock_moves SELECT * FROM other.stock_moves WHERE id = ?1",
            [id],
        )?;
    }
    tx.execute(
        "INSERT INTO main.merge_quarantine (id, kind, reason, payload, at)
         SELECT id, kind, reason, payload, at FROM other.merge_quarantine
         WHERE id NOT IN (SELECT id FROM main.merge_quarantine)",
        [],
    )?;
    tx.execute(
        "INSERT INTO main.audit_log (at, entity, entity_id, action, data, uid)
         SELECT at, entity, entity_id, action, data, uid FROM other.audit_log
         WHERE uid IS NOT NULL
           AND uid NOT IN (SELECT uid FROM main.audit_log WHERE uid IS NOT NULL)
         ORDER BY id",
        [],
    )?;
    tx.execute(
        "INSERT INTO main.number_counters (business_id, year, next)
         SELECT business_id, year, next FROM other.number_counters
         WHERE business_id IN (SELECT id FROM main.businesses)
         ON CONFLICT (business_id, year) DO UPDATE SET next = MAX(next, excluded.next)",
        [],
    )?;
    Ok(())
}

/// Applies the other side's deletion marks.
fn apply_tombstones(
    tx: &Transaction,
    clock: &dyn Clock,
    report: &mut MergeReport,
    has_base: bool,
) -> Result<()> {
    let marks: Vec<(String, Uuid, String)> = {
        let mut stmt = tx.prepare(
            "SELECT o.table_name, o.id, o.at FROM other.tombstones o
             WHERE NOT EXISTS (SELECT 1 FROM main.tombstones m
                               WHERE m.table_name = o.table_name AND m.id = o.id)
             ORDER BY o.at",
        )?;
        stmt.query_map([], |r| Ok((r.get(0)?, uuid_col(r, 1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    for (table, id, at) in marks {
        let known = matches!(
            table.as_str(),
            "businesses" | "brands" | "customers" | "products" | "invoices" | "payments"
        );
        if known && let Some(local_at) = updated_at(tx, "main", &table, id)? {
            let unchanged = if has_base {
                updated_at(tx, "base", &table, id)?.as_deref() == Some(local_at.as_str())
            } else {
                local_at <= at
            };
            let deleted = unchanged
                && !drafts_changed_here(tx, &table, id, has_base, &at)?
                && delete_local(tx, clock, &table, id)?;
            if !deleted {
                // The row stays, so no deletion mark either: later changes
                // to it must still come through.
                report.skipped_deletes.push((table.clone(), id));
                continue;
            }
            report.deleted += 1;
        }
        tx.execute(
            "INSERT INTO main.tombstones (table_name, id, at) VALUES (?1, ?2, ?3)
             ON CONFLICT (table_name, id) DO NOTHING",
            params![table, id.to_string(), at],
        )?;
    }
    Ok(())
}

/// Whether deleting master data row `id` would take drafts with it that
/// were created or changed here since the base (or since the deletion).
fn drafts_changed_here(
    tx: &Transaction,
    table: &str,
    id: Uuid,
    has_base: bool,
    deleted_at: &str,
) -> Result<bool> {
    let column = match table {
        "businesses" => "business_id",
        "brands" => "brand_id",
        "customers" => "customer_id",
        _ => return Ok(false),
    };
    let changed = if has_base {
        "NOT EXISTS (SELECT 1 FROM base.invoices b WHERE b.id = m.id AND b.updated_at = m.updated_at)"
    } else {
        "m.updated_at > ?2"
    };
    let sql = format!(
        "SELECT EXISTS(SELECT 1 FROM main.invoices m
                       WHERE m.{column} = ?1 AND m.state = 'draft' AND {changed})"
    );
    Ok(if has_base {
        tx.query_row(&sql, [id.to_string()], |r| r.get(0))?
    } else {
        tx.query_row(&sql, params![id.to_string(), deleted_at], |r| r.get(0))?
    })
}

/// Parent rows that rows in `main` point at but that are missing.
fn missing_parents(tx: &Transaction) -> Result<Vec<(String, Uuid)>> {
    let violations: Vec<(String, i64, String, i64)> = {
        let mut stmt = tx.prepare("PRAGMA main.foreign_key_check")?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut missing = std::collections::BTreeSet::new();
    for (table, rowid, parent, fkid) in violations {
        let column: String = tx.query_row(
            "SELECT \"from\" FROM pragma_foreign_key_list(?1, 'main') WHERE id = ?2",
            params![table, fkid],
            |r| r.get(0),
        )?;
        let value: Option<String> = tx.query_row(
            &format!("SELECT {column} FROM main.{table} WHERE rowid = ?1"),
            [rowid],
            |r| r.get(0),
        )?;
        if let Some(value) = value {
            let id = Uuid::parse_str(&value)
                .map_err(|_| Error::Merge(format!("{table}.{column} holds no id")))?;
            missing.insert((parent, id));
        }
    }
    Ok(missing.into_iter().collect())
}

/// Rows deleted here that documents of the other side still refer to come
/// back from the other side, and the deletion mark goes. Nothing issued may
/// lose what it refers to.
fn restore_parents(tx: &Transaction, report: &mut MergeReport) -> Result<()> {
    for _ in 0..16 {
        let missing = missing_parents(tx)?;
        if missing.is_empty() {
            return Ok(());
        }
        for (parent, id) in missing {
            let other_state: Option<Option<String>> = if parent == "invoices" {
                tx.query_row(
                    "SELECT state FROM other.invoices WHERE id = ?1",
                    [id.to_string()],
                    |r| r.get(0),
                )
                .optional()?
            } else {
                None
            };
            let restorable = match parent.as_str() {
                "businesses" | "brands" | "customers" | "products" | "payments" => {
                    updated_at(tx, "other", &parent, id)?.is_some()
                }
                "invoices" => other_state.flatten().as_deref() == Some("draft"),
                _ => false,
            };
            if !restorable {
                return Err(Error::Merge(format!("{parent} {id} is missing")));
            }
            tx.execute(
                "DELETE FROM main.tombstones WHERE table_name = ?1 AND id = ?2",
                params![parent, id.to_string()],
            )?;
            copy_row(tx, &parent, &columns(tx, &parent)?, id)?;
            if parent == "invoices" {
                replace_lines(tx, id)?;
            }
            report.restored.push((parent, id));
        }
    }
    Err(Error::Merge("references could not be resolved".into()))
}

/// Deletes one row with the normal rules. `false` when the rules forbid it.
fn delete_local(tx: &Transaction, clock: &dyn Clock, table: &str, id: Uuid) -> Result<bool> {
    use crate::masterdata::{delete_brand, delete_business, delete_customer, delete_product};
    let result = match table {
        "businesses" => delete_business(tx, clock, id),
        "brands" => delete_brand(tx, clock, id),
        "customers" => delete_customer(tx, clock, id),
        "products" => delete_product(tx, clock, id),
        "invoices" => {
            if local_state(tx, id)?.as_deref() != Some("draft") {
                return Ok(false);
            }
            crate::invoice::delete_draft(tx, clock, id)
        }
        "payments" => {
            tx.execute("DELETE FROM main.payments WHERE id = ?1", [id.to_string()])?;
            Ok(())
        }
        _ => return Ok(false),
    };
    match result {
        Ok(()) => Ok(true),
        Err(Error::InUseByIssuedInvoice | Error::NotDraft | Error::NotFound) => Ok(false),
        Err(other) => Err(other),
    }
}

#[cfg(test)]
pub(crate) mod testkit {
    use std::path::{Path, PathBuf};

    use crate::db::Db;

    /// Copies a database to a closed, standalone file.
    pub fn fork(db: &Db, dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        db.read()
            .execute("VACUUM INTO ?1", [path.to_string_lossy().to_string()])
            .unwrap();
        path
    }
}

#[cfg(test)]
mod document_tests {
    use super::testkit::fork;
    use super::*;
    use crate::calc::{LineKind, PriceMode};
    use crate::db::Db;
    use crate::invoice::{InvoiceState, LineDraft, create_draft, load_invoice};
    use crate::issue::{IssueOptions, issue_invoice};
    use crate::masterdata::{
        NewProduct, ProductKind, create_business, create_customer, create_product, get_product,
    };
    use crate::money::Cents;
    use crate::numbering::set_counter_start;
    use crate::payments::{
        PaymentMethod, StatusKind, add_payment, delete_payment, invoice_status, list_payments,
    };
    use crate::storno::cancel_invoice;
    use crate::tax::{TaxCategory, TaxChoice};
    use crate::testutil::{ManualClock, d, sample_business, sample_customer, sample_draft};
    use rust_decimal::dec;

    struct World {
        dir: tempfile::TempDir,
        a: Db,
        clock: ManualClock,
        business: Uuid,
        brand: Uuid,
        customer: Uuid,
    }

    fn world() -> World {
        let dir = tempfile::tempdir().unwrap();
        let clock = ManualClock::new();
        let mut a = Db::open(&dir.path().join("a.psybil")).unwrap();
        let (business, brand) = a
            .write(|tx| create_business(tx, &clock, sample_business()))
            .unwrap();
        let customer = a
            .write(|tx| create_customer(tx, &clock, sample_customer()))
            .unwrap();
        World {
            dir,
            a,
            clock,
            business,
            brand,
            customer,
        }
    }

    fn issue_on(db: &mut Db, w_clock: &ManualClock, brand: Uuid, customer: Uuid) -> Uuid {
        let id = db
            .write(|tx| create_draft(tx, w_clock, sample_draft(brand, Some(customer))))
            .unwrap();
        db.write(|tx| issue_invoice(tx, w_clock, id, IssueOptions::default()))
            .unwrap();
        id
    }

    fn split(w: &World) -> (std::path::PathBuf, std::path::PathBuf) {
        let base = fork(&w.a, w.dir.path(), "base.psybil");
        let b = fork(&w.a, w.dir.path(), "b.psybil");
        w.clock.advance_secs(60);
        (base, b)
    }

    #[test]
    fn merge_adds_other_issued_invoice() {
        let mut w = world();
        let (base, b_path) = split(&w);
        let id = {
            let mut b = Db::open(&b_path).unwrap();
            issue_on(&mut b, &w.clock, w.brand, w.customer)
        };
        w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        let invoice = load_invoice(w.a.read(), id).unwrap();
        assert_eq!(invoice.state, InvoiceState::Issued);
        assert_eq!(invoice.number.as_deref(), Some("RE-2026-0001"));
        assert_eq!(invoice.gross, Some(Cents(2975)));
        assert!(crate::issue::load_snapshot(w.a.read(), id).is_ok());
        assert!(
            w.a.read()
                .execute(
                    "UPDATE invoices SET notes = 'x' WHERE id = ?1",
                    [id.to_string()]
                )
                .is_err()
        );
    }

    #[test]
    fn merge_number_collision_quarantines() {
        let mut w = world();
        let (base, b_path) = split(&w);
        let (brand, customer) = (w.brand, w.customer);
        let local = issue_on(&mut w.a, &w.clock, brand, customer);
        let foreign = {
            let mut b = Db::open(&b_path).unwrap();
            issue_on(&mut b, &w.clock, brand, customer)
        };
        let report = w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert_eq!(
            load_invoice(w.a.read(), local).unwrap().number.as_deref(),
            Some("RE-2026-0001")
        );
        assert!(load_invoice(w.a.read(), foreign).is_err());
        assert_eq!(report.quarantined.len(), 1);
        let reason: String =
            w.a.read()
                .query_row("SELECT reason FROM merge_quarantine", [], |r| r.get(0))
                .unwrap();
        assert_eq!(reason, "number_collision");
    }

    #[test]
    fn merge_payment_deleted_on_one_side_stays_deleted() {
        let mut w = world();
        let (brand, customer) = (w.brand, w.customer);
        let id = issue_on(&mut w.a, &w.clock, brand, customer);
        let clock = &w.clock;
        let payment =
            w.a.write(|tx| {
                add_payment(
                    tx,
                    clock,
                    id,
                    Cents(1000),
                    d(2026, 11, 18),
                    PaymentMethod::Cash,
                    "",
                )
            })
            .unwrap();
        let (base, b_path) = split(&w);
        {
            let mut b = Db::open(&b_path).unwrap();
            b.write(|tx| delete_payment(tx, &w.clock, payment)).unwrap();
        }
        w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert!(list_payments(w.a.read(), id).unwrap().is_empty());
    }

    #[test]
    fn merge_adds_payments_and_audit_once() {
        let mut w = world();
        let (brand, customer) = (w.brand, w.customer);
        let id = issue_on(&mut w.a, &w.clock, brand, customer);
        let (base, b_path) = split(&w);
        {
            let mut b = Db::open(&b_path).unwrap();
            b.write(|tx| {
                add_payment(
                    tx,
                    &w.clock,
                    id,
                    Cents(500),
                    d(2026, 11, 18),
                    PaymentMethod::Cash,
                    "",
                )
            })
            .unwrap();
        }
        w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert_eq!(list_payments(w.a.read(), id).unwrap().len(), 1);
        let created: i64 =
            w.a.read()
                .query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE entity = 'payment' AND action = 'create'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
        assert_eq!(created, 1);
    }

    #[test]
    fn merge_counters_take_maximum() {
        let mut w = world();
        let (base, b_path) = split(&w);
        {
            let mut b = Db::open(&b_path).unwrap();
            b.write(|tx| set_counter_start(tx, &w.clock, w.business, 2026, 10))
                .unwrap();
        }
        w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        let next: i64 =
            w.a.read()
                .query_row(
                    "SELECT next FROM number_counters WHERE year = 2026",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
        assert_eq!(next, 10);
    }

    #[test]
    fn merge_stock_moves_from_both_sides() {
        let mut w = world();
        let brand = w.brand;
        let clock = &w.clock;
        let product =
            w.a.write(|tx| {
                create_product(
                    tx,
                    clock,
                    NewProduct {
                        brand_id: brand,
                        name: "Shirt".into(),
                        description: String::new(),
                        kind: ProductKind::Goods,
                        unit: "Stk".into(),
                        price: dec!(20),
                        price_mode: PriceMode::Net,
                        tax: TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(19) }),
                        stock: Some(dec!(5)),
                        active: true,
                    },
                )
            })
            .unwrap();
        let line = |qty| LineDraft {
            kind: LineKind::Item,
            product_id: Some(product),
            description: "Shirt".into(),
            quantity: qty,
            unit: "Stk".into(),
            unit_price: dec!(20),
            tax: TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(19) }),
        };
        let (base, b_path) = split(&w);
        let issue_lines = |db: &mut Db, qty| {
            let mut input = sample_draft(w.brand, Some(w.customer));
            input.lines = vec![line(qty)];
            let id = db.write(|tx| create_draft(tx, &w.clock, input)).unwrap();
            db.write(|tx| issue_invoice(tx, &w.clock, id, IssueOptions::default()))
                .unwrap();
        };
        {
            let mut b = Db::open(&b_path).unwrap();
            b.write(|tx| set_counter_start(tx, &w.clock, w.business, 2026, 50))
                .unwrap();
            issue_lines(&mut b, dec!(2));
        }
        let mut a = std::mem::replace(&mut w.a, Db::open_in_memory().unwrap());
        issue_lines(&mut a, dec!(1));
        a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert_eq!(get_product(a.read(), product).unwrap().stock, Some(dec!(2)));
    }

    #[test]
    fn merge_cancellation_link_is_taken() {
        let mut w = world();
        let (brand, customer) = (w.brand, w.customer);
        let id = issue_on(&mut w.a, &w.clock, brand, customer);
        let (base, b_path) = split(&w);
        {
            let mut b = Db::open(&b_path).unwrap();
            b.write(|tx| cancel_invoice(tx, &w.clock, id)).unwrap();
        }
        w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert_eq!(
            invoice_status(w.a.read(), id, d(2026, 11, 18))
                .unwrap()
                .kind,
            StatusKind::Cancelled
        );
    }
}

#[cfg(test)]
mod review_tests {
    use super::testkit::fork;
    use super::*;
    use crate::calc::{LineKind, PriceMode};
    use crate::db::Db;
    use crate::invoice::{InvoiceState, LineDraft, create_draft, load_invoice, save_draft};
    use crate::issue::{IssueOptions, issue_invoice};
    use crate::masterdata::{
        NewProduct, ProductKind, create_business, create_customer, create_product, delete_customer,
        get_customer, get_product, update_customer,
    };
    use crate::money::Cents;
    use crate::numbering::set_counter_start;
    use crate::payments::{PaymentMethod, add_payment, delete_payment, list_payments};
    use crate::storno::{cancel_invoice, correct_invoice};
    use crate::tax::{TaxCategory, TaxChoice};
    use crate::testutil::{ManualClock, d, sample_business, sample_customer, sample_draft};
    use crate::tombstone::list_tombstones;
    use rust_decimal::dec;

    struct W {
        dir: tempfile::TempDir,
        a: Db,
        clock: ManualClock,
        business: Uuid,
        brand: Uuid,
        customer: Uuid,
    }

    fn w() -> W {
        let dir = tempfile::tempdir().unwrap();
        let clock = ManualClock::new();
        let mut a = Db::open(&dir.path().join("a.psybil")).unwrap();
        let (business, brand) = a
            .write(|tx| create_business(tx, &clock, sample_business()))
            .unwrap();
        let customer = a
            .write(|tx| create_customer(tx, &clock, sample_customer()))
            .unwrap();
        W {
            dir,
            a,
            clock,
            business,
            brand,
            customer,
        }
    }

    /// Base and B as copies of A, then the clock moves on.
    fn split(w: &W, name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let base = fork(&w.a, w.dir.path(), &format!("{name}-base.psybil"));
        let b = fork(&w.a, w.dir.path(), &format!("{name}.psybil"));
        w.clock.advance_secs(60);
        (base, b)
    }

    fn issue(db: &mut Db, clock: &ManualClock, brand: Uuid, customer: Uuid) -> Uuid {
        let id = db
            .write(|tx| create_draft(tx, clock, sample_draft(brand, Some(customer))))
            .unwrap();
        db.write(|tx| issue_invoice(tx, clock, id, IssueOptions::default()))
            .unwrap();
        id
    }

    fn reasons(db: &Db) -> Vec<String> {
        let mut stmt = db
            .read()
            .prepare("SELECT reason FROM merge_quarantine ORDER BY reason")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    #[test]
    fn customer_deleted_here_but_invoiced_there_comes_back() {
        let mut w = w();
        let (base, b_path) = split(&w, "b");
        let (brand, customer) = (w.brand, w.customer);
        let id = issue(&mut Db::open(&b_path).unwrap(), &w.clock, brand, customer);
        let clock = &w.clock;
        w.a.write(|tx| delete_customer(tx, clock, customer))
            .unwrap();
        let report = w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert_eq!(
            load_invoice(w.a.read(), id).unwrap().state,
            InvoiceState::Issued
        );
        assert!(get_customer(w.a.read(), customer).is_ok());
        assert_eq!(report.restored, vec![("customers".to_string(), customer)]);
    }

    #[test]
    fn storno_of_quarantined_invoice_is_quarantined_too() {
        let mut w = w();
        let (base, b_path) = split(&w, "b");
        let (brand, customer) = (w.brand, w.customer);
        issue(&mut w.a, &w.clock, brand, customer);
        let (foreign, storno) = {
            let mut b = Db::open(&b_path).unwrap();
            let f = issue(&mut b, &w.clock, brand, customer);
            (f, b.write(|tx| cancel_invoice(tx, &w.clock, f)).unwrap())
        };
        let report = w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert!(load_invoice(w.a.read(), foreign).is_err());
        assert!(load_invoice(w.a.read(), storno).is_err());
        assert_eq!(report.quarantined.len(), 2);
    }

    #[test]
    fn correction_of_quarantined_invoice_is_quarantined_too() {
        let mut w = w();
        let (base, b_path) = split(&w, "b");
        let (brand, customer) = (w.brand, w.customer);
        issue(&mut w.a, &w.clock, brand, customer);
        let replacement = {
            let mut b = Db::open(&b_path).unwrap();
            let f = issue(&mut b, &w.clock, brand, customer);
            b.write(|tx| correct_invoice(tx, &w.clock, f)).unwrap().1
        };
        let report = w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert!(load_invoice(w.a.read(), replacement).is_err());
        assert_eq!(report.quarantined.len(), 3);
    }

    #[test]
    fn payment_deleted_here_but_moved_there_comes_back() {
        let mut w = w();
        let (brand, customer) = (w.brand, w.customer);
        let id = issue(&mut w.a, &w.clock, brand, customer);
        let clock = &w.clock;
        let payment =
            w.a.write(|tx| {
                add_payment(
                    tx,
                    clock,
                    id,
                    Cents(1000),
                    d(2026, 11, 18),
                    PaymentMethod::Cash,
                    "",
                )
            })
            .unwrap();
        let (base, b_path) = split(&w, "b");
        let replacement = {
            let mut b = Db::open(&b_path).unwrap();
            let r = b.write(|tx| correct_invoice(tx, &w.clock, id)).unwrap().1;
            b.write(|tx| issue_invoice(tx, &w.clock, r, IssueOptions::default()))
                .unwrap();
            r
        };
        let clock = &w.clock;
        w.a.write(|tx| delete_payment(tx, clock, payment)).unwrap();
        let report = w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert_eq!(list_payments(w.a.read(), replacement).unwrap().len(), 1);
        assert!(report.restored.contains(&("payments".to_string(), payment)));
    }

    fn shirt(w: &mut W) -> Uuid {
        let (brand, clock) = (w.brand, &w.clock);
        w.a.write(|tx| {
            create_product(
                tx,
                clock,
                NewProduct {
                    brand_id: brand,
                    name: "Shirt".into(),
                    description: String::new(),
                    kind: ProductKind::Goods,
                    unit: "Stk".into(),
                    price: dec!(20),
                    price_mode: PriceMode::Net,
                    tax: TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(19) }),
                    stock: Some(dec!(5)),
                    active: true,
                },
            )
        })
        .unwrap()
    }

    #[test]
    fn draft_issued_on_both_sides_quarantines_other_and_books_once() {
        let mut w = w();
        let product = shirt(&mut w);
        let mut input = sample_draft(w.brand, Some(w.customer));
        input.lines = vec![LineDraft {
            kind: LineKind::Item,
            product_id: Some(product),
            description: "Shirt".into(),
            quantity: dec!(1),
            unit: "Stk".into(),
            unit_price: dec!(20),
            tax: TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(19) }),
        }];
        let clock = &w.clock;
        let draft = w.a.write(|tx| create_draft(tx, clock, input)).unwrap();
        let (base, b_path) = split(&w, "b");
        {
            let mut b = Db::open(&b_path).unwrap();
            b.write(|tx| set_counter_start(tx, &w.clock, w.business, 2026, 50))
                .unwrap();
            b.write(|tx| issue_invoice(tx, &w.clock, draft, IssueOptions::default()))
                .unwrap();
        }
        let clock = &w.clock;
        w.a.write(|tx| issue_invoice(tx, clock, draft, IssueOptions::default()))
            .unwrap();
        let report = w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert_eq!(
            load_invoice(w.a.read(), draft).unwrap().number.as_deref(),
            Some("RE-2026-0001")
        );
        assert_eq!(report.quarantined.len(), 1);
        assert_eq!(reasons(&w.a), vec!["issued_on_both_sides"]);
        assert_eq!(
            get_product(w.a.read(), product).unwrap().stock,
            Some(dec!(4))
        );
    }

    #[test]
    fn same_number_with_different_content_is_quarantined() {
        let mut w = w();
        let (brand, customer, clock) = (w.brand, w.customer, &w.clock);
        let draft =
            w.a.write(|tx| create_draft(tx, clock, sample_draft(brand, Some(customer))))
                .unwrap();
        let (base, b_path) = split(&w, "b");
        {
            let mut b = Db::open(&b_path).unwrap();
            let mut input = sample_draft(brand, Some(customer));
            input.lines[0].quantity = dec!(3);
            b.write(|tx| save_draft(tx, &w.clock, draft, input))
                .unwrap();
            b.write(|tx| issue_invoice(tx, &w.clock, draft, IssueOptions::default()))
                .unwrap();
        }
        let clock = &w.clock;
        w.a.write(|tx| issue_invoice(tx, clock, draft, IssueOptions::default()))
            .unwrap();
        let report = w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert_eq!(
            load_invoice(w.a.read(), draft).unwrap().gross,
            Some(Cents(2975))
        );
        assert_eq!(report.quarantined.len(), 1);
    }

    #[test]
    fn invoice_cancelled_on_both_sides_keeps_one_storno() {
        let mut w = w();
        let (brand, customer) = (w.brand, w.customer);
        let id = issue(&mut w.a, &w.clock, brand, customer);
        let (base, b_path) = split(&w, "b");
        let foreign = {
            let mut b = Db::open(&b_path).unwrap();
            b.write(|tx| set_counter_start(tx, &w.clock, w.business, 2026, 50))
                .unwrap();
            b.write(|tx| cancel_invoice(tx, &w.clock, id)).unwrap()
        };
        let clock = &w.clock;
        w.a.write(|tx| cancel_invoice(tx, clock, id)).unwrap();
        let report = w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert!(load_invoice(w.a.read(), foreign).is_err());
        assert_eq!(report.quarantined.len(), 1);
        assert_eq!(reasons(&w.a), vec!["cancelled_on_both_sides"]);
    }

    fn pdf_of(db: &Db, id: Uuid) -> Vec<u8> {
        db.read()
            .query_row(
                "SELECT pdf FROM invoice_snapshots WHERE invoice_id = ?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn merge_carries_archived_pdf() {
        let mut w = w();
        let (base, b_path) = split(&w, "b");
        let (brand, customer) = (w.brand, w.customer);
        let id = issue(&mut Db::open(&b_path).unwrap(), &w.clock, brand, customer);
        let theirs = pdf_of(&Db::open(&b_path).unwrap(), id);
        w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert_eq!(pdf_of(&w.a, id), theirs);
    }

    #[test]
    fn quarantine_keeps_pdf_bytes() {
        use base64::Engine;
        let mut w = w();
        let (base, b_path) = split(&w, "b");
        let (brand, customer) = (w.brand, w.customer);
        issue(&mut w.a, &w.clock, brand, customer);
        let foreign = issue(&mut Db::open(&b_path).unwrap(), &w.clock, brand, customer);
        let theirs = pdf_of(&Db::open(&b_path).unwrap(), foreign);
        w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        let payload: String =
            w.a.read()
                .query_row("SELECT payload FROM merge_quarantine", [], |r| r.get(0))
                .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        let encoded = payload["snapshot"][0]["pdf"]
            .as_str()
            .expect("pdf as base64");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        assert_eq!(decoded, theirs);
    }

    #[test]
    fn quarantine_rows_of_other_side_are_kept() {
        let mut w = w();
        let (base, b_path) = split(&w, "b");
        {
            let mut b = Db::open(&b_path).unwrap();
            b.write(|tx| quarantine(tx, &w.clock, "invoice", "legacy", serde_json::json!({})))
                .unwrap();
        }
        w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert_eq!(reasons(&w.a), vec!["legacy"]);
    }

    #[test]
    fn skipped_delete_does_not_block_later_changes() {
        let mut w = w();
        let (base, b_path) = split(&w, "b");
        let (brand, customer) = (w.brand, w.customer);
        issue(&mut w.a, &w.clock, brand, customer);
        {
            let mut b = Db::open(&b_path).unwrap();
            b.write(|tx| delete_customer(tx, &w.clock, customer))
                .unwrap();
        }
        let report = w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert_eq!(
            report.skipped_deletes,
            vec![("customers".to_string(), customer)]
        );
        assert!(
            !list_tombstones(w.a.read())
                .unwrap()
                .iter()
                .any(|t| t.id == customer)
        );
        let (base2, c_path) = split(&w, "c");
        {
            let mut c = Db::open(&c_path).unwrap();
            let mut row = get_customer(c.read(), customer).unwrap();
            row.city = "Neustadt".into();
            c.write(|tx| update_customer(tx, &w.clock, &row)).unwrap();
        }
        w.a.merge_from(&w.clock, &c_path, Some(&base2)).unwrap();
        assert_eq!(get_customer(w.a.read(), customer).unwrap().city, "Neustadt");
    }

    #[test]
    fn remote_delete_keeps_draft_made_here_since() {
        let mut w = w();
        let (base, b_path) = split(&w, "b");
        let (brand, customer, clock) = (w.brand, w.customer, &w.clock);
        let draft =
            w.a.write(|tx| create_draft(tx, clock, sample_draft(brand, Some(customer))))
                .unwrap();
        {
            let mut b = Db::open(&b_path).unwrap();
            b.write(|tx| delete_customer(tx, &w.clock, customer))
                .unwrap();
        }
        let report = w.a.merge_from(&w.clock, &b_path, Some(&base)).unwrap();
        assert!(load_invoice(w.a.read(), draft).is_ok());
        assert!(get_customer(w.a.read(), customer).is_ok());
        assert_eq!(
            report.skipped_deletes,
            vec![("customers".to_string(), customer)]
        );
    }
}

#[cfg(test)]
mod master_tests {
    use super::testkit::fork;
    use super::*;
    use crate::db::Db;
    use crate::invoice::{InvoiceState, create_draft, load_invoice, save_draft};
    use crate::issue::{IssueOptions, issue_invoice};
    use crate::masterdata::{
        NewCustomer, create_business, create_customer, delete_customer, get_customer,
        update_customer,
    };
    use crate::testutil::{ManualClock, sample_business, sample_customer, sample_draft};

    struct Pair {
        _dir: tempfile::TempDir,
        a: Db,
        b_path: std::path::PathBuf,
        base: std::path::PathBuf,
        clock: ManualClock,
        customer: Uuid,
        brand: Uuid,
    }

    /// A shared start, then A stays open and B is a separate copy.
    fn pair() -> Pair {
        let dir = tempfile::tempdir().unwrap();
        let clock = ManualClock::new();
        let mut a = Db::open(&dir.path().join("a.psybil")).unwrap();
        let (_, brand) = a
            .write(|tx| create_business(tx, &clock, sample_business()))
            .unwrap();
        let customer = a
            .write(|tx| create_customer(tx, &clock, sample_customer()))
            .unwrap();
        let base = fork(&a, dir.path(), "base.psybil");
        let b_path = fork(&a, dir.path(), "b.psybil");
        clock.advance_secs(60);
        Pair {
            _dir: dir,
            a,
            b_path,
            base,
            clock,
            customer,
            brand,
        }
    }

    fn on_b(p: &Pair, f: impl FnOnce(&mut Db)) {
        let mut b = Db::open(&p.b_path).unwrap();
        f(&mut b);
    }

    fn set_city(db: &mut Db, clock: &ManualClock, id: Uuid, city: &str) {
        let mut c = get_customer(db.read(), id).unwrap();
        c.city = city.into();
        db.write(|tx| update_customer(tx, clock, &c)).unwrap();
    }

    #[test]
    fn merge_adds_missing_customer() {
        let mut p = pair();
        let mut added = Uuid::nil();
        on_b(&p, |b| {
            added = b
                .write(|tx| {
                    create_customer(
                        tx,
                        &p.clock,
                        NewCustomer {
                            name: "Kunde B".into(),
                            ..sample_customer()
                        },
                    )
                })
                .unwrap();
        });
        let report = p.a.merge_from(&p.clock, &p.b_path, Some(&p.base)).unwrap();
        assert_eq!(get_customer(p.a.read(), added).unwrap().name, "Kunde B");
        assert!(report.added >= 1);
    }

    #[test]
    fn merge_keeps_local_only_change() {
        let mut p = pair();
        let (customer, clock) = (p.customer, &p.clock);
        set_city(&mut p.a, clock, customer, "Lokalstadt");
        let report = p.a.merge_from(&p.clock, &p.b_path, Some(&p.base)).unwrap();
        assert_eq!(
            get_customer(p.a.read(), p.customer).unwrap().city,
            "Lokalstadt"
        );
        assert!(report.changed_both.is_empty());
    }

    #[test]
    fn merge_takes_other_only_change() {
        let mut p = pair();
        on_b(&p, |b| set_city(b, &p.clock, p.customer, "Fernstadt"));
        p.a.merge_from(&p.clock, &p.b_path, Some(&p.base)).unwrap();
        assert_eq!(
            get_customer(p.a.read(), p.customer).unwrap().city,
            "Fernstadt"
        );
    }

    #[test]
    fn merge_both_changed_newer_wins_and_reports() {
        let mut p = pair();
        let (customer, clock) = (p.customer, &p.clock);
        set_city(&mut p.a, clock, customer, "Lokalstadt");
        p.clock.advance_secs(60);
        on_b(&p, |b| set_city(b, &p.clock, p.customer, "Fernstadt"));
        let report = p.a.merge_from(&p.clock, &p.b_path, Some(&p.base)).unwrap();
        assert_eq!(
            get_customer(p.a.read(), p.customer).unwrap().city,
            "Fernstadt"
        );
        assert_eq!(report.changed_both.len(), 1);
        assert_eq!(report.changed_both[0].kept, Side::Other);
    }

    #[test]
    fn merge_without_base_reports_unclear() {
        let mut p = pair();
        on_b(&p, |b| set_city(b, &p.clock, p.customer, "Fernstadt"));
        let report = p.a.merge_from(&p.clock, &p.b_path, None).unwrap();
        assert_eq!(
            get_customer(p.a.read(), p.customer).unwrap().city,
            "Fernstadt"
        );
        assert_eq!(report.unclear.len(), 1);
    }

    #[test]
    fn merge_applies_other_tombstone() {
        let mut p = pair();
        on_b(&p, |b| {
            b.write(|tx| delete_customer(tx, &p.clock, p.customer))
                .unwrap()
        });
        let report = p.a.merge_from(&p.clock, &p.b_path, Some(&p.base)).unwrap();
        assert!(get_customer(p.a.read(), p.customer).is_err());
        assert_eq!(report.deleted, 1);
    }

    #[test]
    fn merge_does_not_resurrect_locally_deleted() {
        let mut p = pair();
        let (customer, clock) = (p.customer, &p.clock);
        p.a.write(|tx| delete_customer(tx, clock, customer))
            .unwrap();
        p.a.merge_from(&p.clock, &p.b_path, Some(&p.base)).unwrap();
        assert!(get_customer(p.a.read(), p.customer).is_err());
    }

    #[test]
    fn merge_draft_issued_locally_stays_issued() {
        let dir = tempfile::tempdir().unwrap();
        let clock = ManualClock::new();
        let mut a = Db::open(&dir.path().join("a.psybil")).unwrap();
        let (_, brand) = a
            .write(|tx| create_business(tx, &clock, sample_business()))
            .unwrap();
        let customer = a
            .write(|tx| create_customer(tx, &clock, sample_customer()))
            .unwrap();
        let draft = a
            .write(|tx| create_draft(tx, &clock, sample_draft(brand, Some(customer))))
            .unwrap();
        let base = fork(&a, dir.path(), "base.psybil");
        let b_path = fork(&a, dir.path(), "b.psybil");
        clock.advance_secs(60);
        a.write(|tx| issue_invoice(tx, &clock, draft, IssueOptions::default()))
            .unwrap();
        clock.advance_secs(60);
        {
            let mut b = Db::open(&b_path).unwrap();
            let mut input = sample_draft(brand, Some(customer));
            input.notes = "auf B geändert".into();
            b.write(|tx| save_draft(tx, &clock, draft, input)).unwrap();
        }
        a.merge_from(&clock, &b_path, Some(&base)).unwrap();
        let invoice = load_invoice(a.read(), draft).unwrap();
        assert_eq!(invoice.state, InvoiceState::Issued);
        assert_eq!(invoice.notes, "");
    }

    #[test]
    fn merge_refuses_newer_schema() {
        let mut p = pair();
        let newer = p.b_path.with_file_name("newer.psybil");
        {
            let conn = rusqlite::Connection::open(&newer).unwrap();
            conn.execute_batch("CREATE TABLE x (a); PRAGMA user_version = 3;")
                .unwrap();
        }
        let _ = p.brand;
        assert!(matches!(
            p.a.merge_from(&p.clock, &newer, None),
            Err(crate::Error::NewerSchema { found: 3, .. })
        ));
    }
}
