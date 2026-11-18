//! Append-only audit log (GoBD: changes must stay traceable).

use rusqlite::{Transaction, params};
use uuid::Uuid;

use crate::Result;
use crate::dates::{Clock, timestamp};

/// Appends one entry to the audit log.
pub fn log(
    tx: &Transaction,
    clock: &dyn Clock,
    entity: &str,
    entity_id: Uuid,
    action: &str,
    data: serde_json::Value,
) -> Result<()> {
    tx.execute(
        "INSERT INTO audit_log (at, entity, entity_id, action, data, uid)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            timestamp(clock),
            entity,
            entity_id.to_string(),
            action,
            data.to_string(),
            Uuid::new_v4().to_string()
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::FixedClock;
    use crate::db::Db;
    use chrono::NaiveDate;

    #[test]
    fn audit_rows_are_append_only() {
        let mut db = Db::open_in_memory().unwrap();
        let clock = FixedClock {
            today: NaiveDate::from_ymd_opt(2026, 11, 18).unwrap(),
        };
        let id = Uuid::new_v4();
        db.write(|tx| {
            log(
                tx,
                &clock,
                "customer",
                id,
                "update",
                serde_json::json!({"a": 1}),
            )
        })
        .unwrap();
        let (entity, action, at): (String, String, String) = db
            .read()
            .query_row("SELECT entity, action, at FROM audit_log", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!((entity.as_str(), action.as_str()), ("customer", "update"));
        assert!(at.starts_with("2026-11-18"));
        assert!(
            db.read()
                .execute("UPDATE audit_log SET action = 'x'", [])
                .is_err()
        );
        assert!(db.read().execute("DELETE FROM audit_log", []).is_err());
    }
}
