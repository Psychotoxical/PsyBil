//! Deletion marks. A merge needs to know what was deleted on purpose, or it
//! would bring the row back from the other copy.

use rusqlite::{Connection, Transaction, params};
use uuid::Uuid;

use crate::Result;
use crate::dates::{Clock, timestamp};
use crate::db::uuid_col;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tombstone {
    pub table: String,
    pub id: Uuid,
    pub at: String,
}

/// Remembers that a row was deleted.
pub(crate) fn mark_deleted(
    tx: &Transaction,
    clock: &dyn Clock,
    table: &str,
    id: Uuid,
) -> Result<()> {
    tx.execute(
        "INSERT INTO tombstones (table_name, id, at) VALUES (?1, ?2, ?3)
         ON CONFLICT (table_name, id) DO UPDATE SET at = excluded.at",
        params![table, id.to_string(), timestamp(clock)],
    )?;
    Ok(())
}

/// Deletes all drafts whose `column` equals `id`, with deletion marks.
/// Only drafts: issued invoices are checked for by the callers first.
pub(crate) fn delete_drafts_where(
    tx: &Transaction,
    clock: &dyn Clock,
    column: &str,
    id: Uuid,
) -> Result<()> {
    let mut stmt = tx.prepare(&format!(
        "SELECT id FROM invoices WHERE {column} = ?1 AND state = 'draft'"
    ))?;
    let drafts = stmt
        .query_map([id.to_string()], |r| uuid_col(r, 0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for draft in drafts {
        tx.execute("DELETE FROM invoices WHERE id = ?1", [draft.to_string()])?;
        mark_deleted(tx, clock, "invoices", draft)?;
    }
    Ok(())
}

pub fn list_tombstones(conn: &Connection) -> Result<Vec<Tombstone>> {
    let mut stmt = conn.prepare("SELECT table_name, id, at FROM tombstones ORDER BY at")?;
    let rows = stmt.query_map([], |r| {
        Ok(Tombstone {
            table: r.get(0)?,
            id: uuid_col(r, 1)?,
            at: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::invoice::{create_draft, delete_draft};
    use crate::issue::{IssueOptions, issue_invoice};
    use crate::masterdata::{create_business, create_customer, delete_customer};
    use crate::money::Cents;
    use crate::payments::{PaymentMethod, add_payment, delete_payment, list_payments};
    use crate::storno::correct_invoice;
    use crate::testutil::{clock, d, sample_business, sample_customer, sample_draft};

    fn marked(db: &Db, table: &str) -> Vec<Uuid> {
        list_tombstones(db.read())
            .unwrap()
            .into_iter()
            .filter(|t| t.table == table)
            .map(|t| t.id)
            .collect()
    }

    #[test]
    fn deleting_customer_marks_customer_and_drafts() {
        let mut db = Db::open_in_memory().unwrap();
        let (_, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let customer = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        let draft = db
            .write(|tx| create_draft(tx, &clock(), sample_draft(brand_id, Some(customer))))
            .unwrap();
        db.write(|tx| delete_customer(tx, &clock(), customer))
            .unwrap();
        assert_eq!(marked(&db, "customers"), vec![customer]);
        assert_eq!(marked(&db, "invoices"), vec![draft]);
    }

    #[test]
    fn delete_draft_marks_invoice() {
        let mut db = Db::open_in_memory().unwrap();
        let (_, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let draft = db
            .write(|tx| create_draft(tx, &clock(), sample_draft(brand_id, None)))
            .unwrap();
        db.write(|tx| delete_draft(tx, &clock(), draft)).unwrap();
        assert_eq!(marked(&db, "invoices"), vec![draft]);
    }

    #[test]
    fn deleting_payment_copy_marks_both() {
        let mut db = Db::open_in_memory().unwrap();
        let (_, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let customer = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        let id = db
            .write(|tx| create_draft(tx, &clock(), sample_draft(brand_id, Some(customer))))
            .unwrap();
        db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()))
            .unwrap();
        let original = db
            .write(|tx| {
                add_payment(
                    tx,
                    &clock(),
                    id,
                    Cents(2975),
                    d(2026, 11, 18),
                    PaymentMethod::Cash,
                    "",
                )
            })
            .unwrap();
        let (_, replacement) = db.write(|tx| correct_invoice(tx, &clock(), id)).unwrap();
        db.write(|tx| issue_invoice(tx, &clock(), replacement, IssueOptions::default()))
            .unwrap();
        let copy = list_payments(db.read(), replacement).unwrap().remove(0).id;
        db.write(|tx| delete_payment(tx, &clock(), copy)).unwrap();
        let mut payments = marked(&db, "payments");
        payments.sort();
        let mut expected = vec![copy, original];
        expected.sort();
        assert_eq!(payments, expected);
    }
}
