//! Invoice numbers: one sequence per business and calendar year, shared by
//! all brands and cancellations of that business, so no number can repeat.

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use uuid::Uuid;

use crate::audit;
use crate::dates::Clock;
use crate::{Error, Result};

/// Fills a number template. Placeholders: `{PREFIX}`, `{YYYY}`, `{YY}`,
/// `{NNNN}` (at least four digits) and `{N}`.
pub fn format_number(template: &str, prefix: &str, year: i32, n: u32) -> String {
    template
        .replace("{PREFIX}", prefix)
        .replace("{YYYY}", &format!("{year:04}"))
        .replace("{YY}", &format!("{:02}", year.rem_euclid(100)))
        .replace("{NNNN}", &format!("{n:04}"))
        .replace("{N}", &n.to_string())
}

fn counter(conn: &Connection, business_id: Uuid, year: i32) -> Result<Option<u32>> {
    Ok(conn
        .query_row(
            "SELECT next FROM number_counters WHERE business_id = ?1 AND year = ?2",
            params![business_id.to_string(), year],
            |r| r.get(0),
        )
        .optional()?)
}

fn store_counter(tx: &Transaction, business_id: Uuid, year: i32, next: u32) -> Result<()> {
    tx.execute(
        "INSERT INTO number_counters (business_id, year, next) VALUES (?1, ?2, ?3)
         ON CONFLICT (business_id, year) DO UPDATE SET next = excluded.next",
        params![business_id.to_string(), year, next],
    )?;
    Ok(())
}

/// Takes the next free number of the business for `year`. Numbers that
/// already exist (for example imported ones) are skipped.
pub fn allocate_number(
    tx: &Transaction,
    business_id: Uuid,
    prefix: &str,
    year: i32,
) -> Result<String> {
    let template: String = tx
        .query_row(
            "SELECT number_template FROM businesses WHERE id = ?1",
            [business_id.to_string()],
            |r| r.get(0),
        )
        .map_err(crate::db::not_found)?;
    let mut n = counter(tx, business_id, year)?.unwrap_or(1);
    loop {
        let candidate = format_number(&template, prefix, year, n);
        let taken: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM invoices WHERE business_id = ?1 AND number = ?2)",
            params![business_id.to_string(), candidate],
            |r| r.get(0),
        )?;
        n = n
            .checked_add(1)
            .ok_or(Error::CounterTooLow { existing: u32::MAX })?;
        if !taken {
            store_counter(tx, business_id, year, n)?;
            return Ok(candidate);
        }
    }
}

/// Sets the next number for a year, for continuing an existing sequence.
/// It can only move forward.
pub fn set_counter_start(
    tx: &Transaction,
    clock: &dyn Clock,
    business_id: Uuid,
    year: i32,
    next: u32,
) -> Result<()> {
    let current = counter(tx, business_id, year)?.unwrap_or(1);
    if next < current {
        return Err(Error::CounterTooLow {
            existing: current - 1,
        });
    }
    store_counter(tx, business_id, year, next)?;
    audit::log(
        tx,
        clock,
        "business",
        business_id,
        "counter_start",
        serde_json::json!({ "year": year, "next": next }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::masterdata::{NewBusiness, create_business};
    use crate::testutil::{clock, insert_raw_invoice, sample_business};

    fn setup() -> (Db, Uuid, Uuid) {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        (db, business_id, brand_id)
    }

    #[test]
    fn format_default() {
        assert_eq!(
            format_number("{PREFIX}-{YYYY}-{NNNN}", "RE", 2026, 1),
            "RE-2026-0001"
        );
        assert_eq!(format_number("{YY}/{N}", "RE", 2026, 42), "26/42");
        assert_eq!(format_number("{PREFIX}{NNNN}", "X", 2026, 12345), "X12345");
    }

    #[test]
    fn brands_share_counter() {
        let (mut db, business_id, _) = setup();
        let a = db
            .write(|tx| allocate_number(tx, business_id, "RE", 2026))
            .unwrap();
        let b = db
            .write(|tx| allocate_number(tx, business_id, "BAND", 2026))
            .unwrap();
        assert_eq!((a.as_str(), b.as_str()), ("RE-2026-0001", "BAND-2026-0002"));
    }

    #[test]
    fn businesses_have_separate_counters() {
        let (mut db, first, _) = setup();
        let (second, _) = db
            .write(|tx| {
                create_business(
                    tx,
                    &clock(),
                    NewBusiness {
                        name: "Zweite".into(),
                        ..sample_business()
                    },
                )
            })
            .unwrap();
        db.write(|tx| allocate_number(tx, first, "RE", 2026))
            .unwrap();
        let other = db
            .write(|tx| allocate_number(tx, second, "RE", 2026))
            .unwrap();
        assert_eq!(other, "RE-2026-0001");
    }

    #[test]
    fn year_from_argument() {
        let (mut db, business_id, _) = setup();
        db.write(|tx| allocate_number(tx, business_id, "RE", 2026))
            .unwrap();
        let old = db
            .write(|tx| allocate_number(tx, business_id, "RE", 2025))
            .unwrap();
        assert_eq!(old, "RE-2025-0001");
    }

    #[test]
    fn skips_existing_number() {
        let (mut db, business_id, brand_id) = setup();
        let id = insert_raw_invoice(db.read(), business_id, brand_id, None, "draft");
        db.read()
            .execute(
                "UPDATE invoices SET state = 'issued', number = 'RE-2026-0001' WHERE id = ?1",
                [id.to_string()],
            )
            .unwrap();
        let next = db
            .write(|tx| allocate_number(tx, business_id, "RE", 2026))
            .unwrap();
        assert_eq!(next, "RE-2026-0002");
    }

    #[test]
    fn counter_start_cannot_go_down() {
        let (mut db, business_id, _) = setup();
        db.write(|tx| allocate_number(tx, business_id, "RE", 2026))
            .unwrap();
        db.write(|tx| allocate_number(tx, business_id, "RE", 2026))
            .unwrap();
        let low = db.write(|tx| set_counter_start(tx, &clock(), business_id, 2026, 2));
        assert!(matches!(low, Err(Error::CounterTooLow { existing: 2 })));
        db.write(|tx| set_counter_start(tx, &clock(), business_id, 2026, 10))
            .unwrap();
        let next = db
            .write(|tx| allocate_number(tx, business_id, "RE", 2026))
            .unwrap();
        assert_eq!(next, "RE-2026-0010");
    }

    #[test]
    fn counter_start_for_new_year() {
        let (mut db, business_id, _) = setup();
        db.write(|tx| set_counter_start(tx, &clock(), business_id, 2027, 50))
            .unwrap();
        let next = db
            .write(|tx| allocate_number(tx, business_id, "RE", 2027))
            .unwrap();
        assert_eq!(next, "RE-2027-0050");
    }
}
