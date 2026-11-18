//! The overview page: what is still open, what came in, and how close a
//! small business is to the limits of § 19 UStG.

use chrono::{Datelike, NaiveDate};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::dates::{Clock, timestamp};
use crate::money::Cents;
use crate::validate::{Issue, IssueKind, Severity};
use crate::{Error, Result};

/// Limit for the previous year: 25.000,00 €.
pub const PREVIOUS_LIMIT: Cents = Cents(2_500_000);
/// Limit for the current year: 100.000,00 €.
pub const CURRENT_LIMIT: Cents = Cents(10_000_000);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct OutsideRevenue {
    pub year: i32,
    pub amount: Cents,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum Level {
    Ok,
    /// 80 % of the limit or more.
    Warning,
    /// Above the limit.
    Over,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct SmallBusinessMeter {
    pub business_id: Uuid,
    pub business_name: String,
    pub year: i32,
    pub previous: Cents,
    pub previous_level: Level,
    pub current: Cents,
    /// The current year against 100.000 €: above it, the rule ends at once.
    pub current_level: Level,
    /// The current year against 25.000 €: above it, the rule does not apply
    /// next year.
    pub next_year_level: Level,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct CurrencyTiles {
    pub currency: String,
    pub open: Cents,
    pub open_count: u32,
    pub overdue: Cents,
    pub overdue_count: u32,
    pub received_this_month: Cents,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Debtor {
    pub invoice_id: Uuid,
    pub number: String,
    pub customer_name: String,
    pub invoice_date: NaiveDate,
    pub due_date: NaiveDate,
    /// Days since the due date; negative while it is not due yet.
    #[cfg_attr(test, ts(type = "number"))]
    pub days_overdue: i64,
    pub open: Cents,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MonthlyReceipts {
    pub currency: String,
    /// Twelve entries, January first.
    pub months: Vec<Cents>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Overview {
    /// The year of the monthly receipts and the meters.
    pub year: i32,
    pub tiles: Vec<CurrencyTiles>,
    pub debtors: Vec<Debtor>,
    pub monthly: Vec<MonthlyReceipts>,
    pub meters: Vec<SmallBusinessMeter>,
}

/// How `amount` stands against `limit`.
pub fn level(amount: Cents, limit: Cents) -> Level {
    if amount.0 > limit.0 {
        Level::Over
    } else if i128::from(amount.0) * 5 >= i128::from(limit.0) * 4 {
        Level::Warning
    } else {
        Level::Ok
    }
}

/// The row id for one business and year; the same on every device.
fn outside_revenue_id(business_id: Uuid, year: i32) -> Uuid {
    Uuid::new_v5(&business_id, year.to_string().as_bytes())
}

/// Sets the revenue outside PsyBil for one business and year.
pub fn set_outside_revenue(
    tx: &Transaction,
    clock: &dyn Clock,
    business_id: Uuid,
    year: i32,
    amount: Cents,
    note: &str,
) -> Result<()> {
    if amount < Cents::ZERO {
        return Err(Error::Validation(vec![Issue {
            kind: IssueKind::NegativeTotal,
            severity: Severity::Error,
        }]));
    }
    crate::masterdata::get_business(tx, business_id)?;
    let now = timestamp(clock);
    tx.execute(
        "INSERT INTO outside_revenue
            (id, business_id, year, amount_cents, note, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(id) DO UPDATE SET amount_cents = excluded.amount_cents,
             note = excluded.note, updated_at = excluded.updated_at",
        params![
            outside_revenue_id(business_id, year).to_string(),
            business_id.to_string(),
            year,
            amount.0,
            note,
            now,
        ],
    )?;
    crate::audit::log(
        tx,
        clock,
        "business",
        business_id,
        "set_outside_revenue",
        serde_json::json!({ "year": year, "amount_cents": amount.0 }),
    )
}

pub fn outside_revenue(
    conn: &Connection,
    business_id: Uuid,
    year: i32,
) -> Result<Option<OutsideRevenue>> {
    Ok(conn
        .query_row(
            "SELECT amount_cents, note FROM outside_revenue WHERE business_id = ?1 AND year = ?2",
            params![business_id.to_string(), year],
            |r| {
                Ok(OutsideRevenue {
                    year,
                    amount: Cents(r.get(0)?),
                    note: r.get(1)?,
                })
            },
        )
        .optional()?)
}

/// The customer name as printed on an issued document.
pub(crate) fn printed_customer_name(conn: &Connection, invoice_id: Uuid) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT json_extract(snapshot, '$.customer.name') FROM invoice_snapshots
             WHERE invoice_id = ?1",
            [invoice_id.to_string()],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}

pub(crate) fn year_bounds(year: i32) -> (NaiveDate, NaiveDate) {
    (
        NaiveDate::from_ymd_opt(year, 1, 1).expect("valid year"),
        NaiveDate::from_ymd_opt(year, 12, 31).expect("valid year"),
    )
}

/// Receipts of one business in `year` plus what was entered by hand.
fn year_revenue(conn: &Connection, business_id: Uuid, year: i32) -> Result<Cents> {
    let (from, to) = year_bounds(year);
    let received = crate::payments::receipts(conn, Some(business_id), from, to)?;
    let outside = outside_revenue(conn, business_id, year)?.map_or(Cents::ZERO, |o| o.amount);
    Ok(received.checked_add(outside)?)
}

fn meters(
    conn: &Connection,
    business: Option<Uuid>,
    today: NaiveDate,
) -> Result<Vec<SmallBusinessMeter>> {
    let year = today.year();
    let mut stmt = conn.prepare(
        "SELECT id, name FROM businesses
         WHERE small_business = 1 AND archived = 0 AND (?1 IS NULL OR id = ?1)
         ORDER BY name COLLATE NOCASE, id",
    )?;
    let rows: Vec<(Uuid, String)> = stmt
        .query_map([business.map(|b| b.to_string())], |r| {
            Ok((crate::db::uuid_col(r, 0)?, r.get(1)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    rows.into_iter()
        .map(|(business_id, business_name)| {
            let previous = year_revenue(conn, business_id, year - 1)?;
            let current = year_revenue(conn, business_id, year)?;
            Ok(SmallBusinessMeter {
                business_id,
                business_name,
                year,
                previous,
                previous_level: level(previous, PREVIOUS_LIMIT),
                current,
                current_level: level(current, CURRENT_LIMIT),
                next_year_level: level(current, PREVIOUS_LIMIT),
            })
        })
        .collect()
}

/// Currencies to show: those of issued documents, else those of the
/// businesses, else euro.
fn currencies(conn: &Connection, business: Option<Uuid>) -> Result<Vec<String>> {
    let scope = business.map(|b| b.to_string());
    let mut stmt = conn.prepare(
        "SELECT DISTINCT currency FROM invoices
         WHERE state = 'issued' AND (?1 IS NULL OR business_id = ?1) ORDER BY currency",
    )?;
    let mut out: Vec<String> = stmt
        .query_map([&scope], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    if out.is_empty() {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT currency FROM businesses
             WHERE archived = 0 AND (?1 IS NULL OR id = ?1) ORDER BY currency",
        )?;
        out = stmt
            .query_map([&scope], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
    }
    if out.is_empty() {
        out.push("EUR".into());
    }
    Ok(out)
}

/// Receipts per currency and month of `year`; a payment moved to a
/// replacement invoice counts once, on its original date.
fn monthly(
    conn: &Connection,
    business: Option<Uuid>,
    year: i32,
    currencies: &[String],
) -> Result<Vec<MonthlyReceipts>> {
    let (from, to) = year_bounds(year);
    let mut out: Vec<MonthlyReceipts> = currencies
        .iter()
        .map(|c| MonthlyReceipts {
            currency: c.clone(),
            months: vec![Cents::ZERO; 12],
        })
        .collect();
    let mut stmt = conn.prepare(
        "SELECT i.currency, CAST(strftime('%m', p.date) AS INTEGER), SUM(p.amount_cents)
         FROM payments p JOIN invoices i ON i.id = p.invoice_id
         WHERE p.transfer_of_id IS NULL AND p.date BETWEEN ?1 AND ?2
           AND (?3 IS NULL OR i.business_id = ?3)
         GROUP BY 1, 2",
    )?;
    let rows: Vec<(String, u32, i64)> = stmt
        .query_map(params![from, to, business.map(|b| b.to_string())], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    for (currency, month, cents) in rows {
        let index = match out.iter().position(|m| m.currency == currency) {
            Some(i) => i,
            None => {
                out.push(MonthlyReceipts {
                    currency,
                    months: vec![Cents::ZERO; 12],
                });
                out.len() - 1
            }
        };
        if let Some(slot) = out[index].months.get_mut((month as usize).wrapping_sub(1)) {
            *slot = slot.checked_add(Cents(cents))?;
        }
    }
    Ok(out)
}

/// Everything the overview page shows, for one business or all.
pub fn overview(conn: &Connection, business: Option<Uuid>, today: NaiveDate) -> Result<Overview> {
    let currencies = currencies(conn, business)?;
    let mut tiles: Vec<CurrencyTiles> = currencies
        .iter()
        .map(|c| CurrencyTiles {
            currency: c.clone(),
            open: Cents::ZERO,
            open_count: 0,
            overdue: Cents::ZERO,
            overdue_count: 0,
            received_this_month: Cents::ZERO,
        })
        .collect();
    let mut stmt = conn.prepare(
        "SELECT id FROM invoices
         WHERE state = 'issued' AND kind = 'invoice' AND (?1 IS NULL OR business_id = ?1)",
    )?;
    let ids: Vec<Uuid> = stmt
        .query_map([business.map(|b| b.to_string())], |r| {
            crate::db::uuid_col(r, 0)
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut debtors = Vec::new();
    for id in ids {
        let status = crate::payments::invoice_status(conn, id, today)?;
        if status.open <= Cents::ZERO {
            continue;
        }
        let invoice = crate::invoice::load_invoice(conn, id)?;
        if let Some(tile) = tiles.iter_mut().find(|t| t.currency == invoice.currency) {
            tile.open = tile.open.checked_add(status.open)?;
            tile.open_count += 1;
            if status.overdue {
                tile.overdue = tile.overdue.checked_add(status.open)?;
                tile.overdue_count += 1;
            }
        }
        debtors.push(Debtor {
            invoice_id: id,
            number: invoice.number.clone().unwrap_or_default(),
            customer_name: printed_customer_name(conn, id)?.unwrap_or_default(),
            invoice_date: invoice.invoice_date,
            due_date: invoice.due_date,
            days_overdue: (today - invoice.due_date).num_days(),
            open: status.open,
            currency: invoice.currency,
        });
    }
    debtors.sort_by(|a, b| {
        a.due_date
            .cmp(&b.due_date)
            .then_with(|| a.number.cmp(&b.number))
    });
    let monthly = monthly(conn, business, today.year(), &currencies)?;
    let month = today.month() as usize - 1;
    for m in &monthly {
        if let Some(tile) = tiles.iter_mut().find(|t| t.currency == m.currency) {
            tile.received_this_month = m.months[month];
        }
    }
    Ok(Overview {
        year: today.year(),
        tiles,
        debtors,
        monthly,
        meters: meters(conn, business, today)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::invoice::{DraftInput, create_draft};
    use crate::issue::{IssueOptions, issue_invoice};
    use crate::masterdata::{
        NewBusiness, archive_business, create_business, create_customer, get_customer,
        update_customer,
    };
    use crate::payments::{PaymentMethod, add_payment};
    use crate::tax::{TaxCategory, TaxChoice};
    use crate::testutil::{ManualClock, clock, d, sample_business, sample_customer, sample_draft};
    use rust_decimal::Decimal;
    use rust_decimal::dec;

    const TODAY: (i32, u32, u32) = (2026, 11, 18);

    fn today() -> NaiveDate {
        d(TODAY.0, TODAY.1, TODAY.2)
    }

    struct World {
        db: Db,
        business: Uuid,
        brand: Uuid,
        customer: Uuid,
    }

    fn world_with(input: NewBusiness) -> World {
        let mut db = Db::open_in_memory().unwrap();
        let (business, brand) = db.write(|tx| create_business(tx, &clock(), input)).unwrap();
        let customer = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        World {
            db,
            business,
            brand,
            customer,
        }
    }

    fn small_world() -> World {
        world_with(NewBusiness {
            small_business: true,
            small_business_since: Some(d(2020, 1, 1)),
            ..sample_business()
        })
    }

    fn draft_input(w: &World, brand: Uuid, date: NaiveDate, price: Decimal) -> DraftInput {
        let small = crate::masterdata::get_business(w.db.read(), w.business)
            .map(|b| b.small_business && brand == w.brand)
            .unwrap_or(false);
        let mut input = sample_draft(brand, Some(w.customer));
        input.invoice_date = date;
        input.lines[0].quantity = dec!(1);
        input.lines[0].unit_price = price;
        if small {
            input.lines[0].tax = TaxChoice::Fixed(TaxCategory::SmallBusiness);
        }
        input
    }

    /// An issued invoice of `price` dated `date`, on the world's brand.
    fn issue(w: &mut World, date: NaiveDate, price: Decimal) -> Uuid {
        let input = draft_input(w, w.brand, date, price);
        issue_with(w, input)
    }

    fn issue_with(w: &mut World, input: DraftInput) -> Uuid {
        w.db.write(|tx| {
            let id = create_draft(tx, &clock(), input)?;
            issue_invoice(
                tx,
                &clock(),
                id,
                IssueOptions {
                    payment_received: false,
                },
            )?;
            Ok(id)
        })
        .unwrap()
    }

    fn pay(w: &mut World, id: Uuid, cents: i64, date: NaiveDate) {
        w.db.write(|tx| {
            add_payment(
                tx,
                &clock(),
                id,
                Cents(cents),
                date,
                PaymentMethod::Transfer,
                "",
            )
        })
        .unwrap();
    }

    fn meter(w: &World) -> SmallBusinessMeter {
        let o = overview(w.db.read(), Some(w.business), today()).unwrap();
        assert_eq!(o.meters.len(), 1);
        o.meters[0].clone()
    }

    #[test]
    fn meter_counts_receipts_of_the_year_plus_outside_revenue() {
        let mut w = small_world();
        let a = issue(&mut w, d(2026, 3, 10), dec!(1000));
        pay(&mut w, a, 100_000, d(2026, 3, 20));
        let b = issue(&mut w, d(2026, 11, 2), dec!(80));
        pay(&mut w, b, 3_000, d(2026, 11, 10));
        w.db.write(|tx| {
            set_outside_revenue(tx, &clock(), w.business, 2026, Cents(50_000), "Merch")
        })
        .unwrap();
        w.db.write(|tx| set_outside_revenue(tx, &clock(), w.business, 2025, Cents(20_000), ""))
            .unwrap();
        let m = meter(&w);
        assert_eq!(m.year, 2026);
        assert_eq!(m.current, Cents(153_000));
        assert_eq!(m.previous, Cents(20_000));
        assert_eq!(m.business_name, "Beispiel GbR");
        // An open invoice does not count until it is paid.
        issue(&mut w, d(2026, 11, 3), dec!(500));
        assert_eq!(meter(&w).current, Cents(153_000));
    }

    #[test]
    fn meter_levels_at_the_limits() {
        assert_eq!(level(Cents(1_999_999), PREVIOUS_LIMIT), Level::Ok);
        assert_eq!(level(Cents(2_000_000), PREVIOUS_LIMIT), Level::Warning);
        assert_eq!(level(Cents(2_500_000), PREVIOUS_LIMIT), Level::Warning);
        assert_eq!(level(Cents(2_500_001), PREVIOUS_LIMIT), Level::Over);
        assert_eq!(level(Cents(7_999_999), CURRENT_LIMIT), Level::Ok);
        assert_eq!(level(Cents(8_000_000), CURRENT_LIMIT), Level::Warning);
        assert_eq!(level(Cents(10_000_000), CURRENT_LIMIT), Level::Warning);
        assert_eq!(level(Cents(10_000_001), CURRENT_LIMIT), Level::Over);
    }

    #[test]
    fn meter_splits_previous_and_current_year() {
        let mut w = small_world();
        let a = issue(&mut w, d(2025, 12, 1), dec!(500));
        pay(&mut w, a, 30_000, d(2025, 12, 31));
        pay(&mut w, a, 20_000, d(2026, 1, 1));
        w.db.write(|tx| set_outside_revenue(tx, &clock(), w.business, 2025, Cents(2_000_000), ""))
            .unwrap();
        let m = meter(&w);
        assert_eq!(m.previous, Cents(2_030_000));
        assert_eq!(m.previous_level, Level::Warning);
        assert_eq!(m.current, Cents(20_000));
        assert_eq!(m.current_level, Level::Ok);
        assert_eq!(m.next_year_level, Level::Ok);
    }

    #[test]
    fn current_year_against_25k_decides_next_year() {
        let mut w = small_world();
        w.db.write(|tx| set_outside_revenue(tx, &clock(), w.business, 2026, Cents(2_100_000), ""))
            .unwrap();
        let m = meter(&w);
        assert_eq!(m.current_level, Level::Ok);
        assert_eq!(m.next_year_level, Level::Warning);
        w.db.write(|tx| set_outside_revenue(tx, &clock(), w.business, 2026, Cents(2_500_001), ""))
            .unwrap();
        assert_eq!(meter(&w).next_year_level, Level::Over);
    }

    #[test]
    fn transferred_payment_counts_once() {
        let mut w = small_world();
        let a = issue(&mut w, d(2026, 5, 4), dec!(200));
        pay(&mut w, a, 10_000, d(2026, 5, 20));
        let (_, replacement) =
            w.db.write(|tx| crate::storno::correct_invoice(tx, &clock(), a))
                .unwrap();
        w.db.write(|tx| {
            issue_invoice(
                tx,
                &clock(),
                replacement,
                IssueOptions {
                    payment_received: false,
                },
            )
        })
        .unwrap();
        assert_eq!(meter(&w).current, Cents(10_000));
    }

    #[test]
    fn meter_only_for_small_businesses() {
        let mut w = world_with(sample_business());
        let o = overview(w.db.read(), None, today()).unwrap();
        assert!(o.meters.is_empty());
        let (small, _) =
            w.db.write(|tx| {
                create_business(
                    tx,
                    &clock(),
                    NewBusiness {
                        name: "Muster Studio".into(),
                        small_business: true,
                        ..sample_business()
                    },
                )
            })
            .unwrap();
        let o = overview(w.db.read(), None, today()).unwrap();
        assert_eq!(o.meters.len(), 1);
        assert_eq!(o.meters[0].business_id, small);
        w.db.write(|tx| archive_business(tx, &clock(), small, true))
            .unwrap();
        assert!(
            overview(w.db.read(), None, today())
                .unwrap()
                .meters
                .is_empty()
        );
    }

    #[test]
    fn outside_revenue_is_one_sum_per_business_and_year() {
        let mut w = small_world();
        w.db.write(|tx| set_outside_revenue(tx, &clock(), w.business, 2026, Cents(10_000), "a"))
            .unwrap();
        w.db.write(|tx| set_outside_revenue(tx, &clock(), w.business, 2026, Cents(25_000), "b"))
            .unwrap();
        let got = outside_revenue(w.db.read(), w.business, 2026).unwrap();
        assert_eq!(
            got,
            Some(OutsideRevenue {
                year: 2026,
                amount: Cents(25_000),
                note: "b".into(),
            })
        );
        let rows: i64 =
            w.db.read()
                .query_row("SELECT COUNT(*) FROM outside_revenue", [], |r| r.get(0))
                .unwrap();
        assert_eq!(rows, 1);
        assert_eq!(
            outside_revenue(w.db.read(), w.business, 2025).unwrap(),
            None
        );
    }

    #[test]
    fn outside_revenue_merges_without_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let (pa, pb) = (dir.path().join("a.db"), dir.path().join("b.db"));
        let early = ManualClock::new();
        let mut a = Db::open(&pa).unwrap();
        let (business, _) = a
            .write(|tx| create_business(tx, &early, sample_business()))
            .unwrap();
        a.read()
            .execute("VACUUM INTO ?1", [pb.to_string_lossy().to_string()])
            .unwrap();
        let mut b = Db::open(&pb).unwrap();
        a.write(|tx| set_outside_revenue(tx, &early, business, 2026, Cents(10_000), "A"))
            .unwrap();
        let later = ManualClock::new();
        later.advance_secs(60);
        b.write(|tx| set_outside_revenue(tx, &later, business, 2026, Cents(30_000), "B"))
            .unwrap();
        drop(b);
        a.merge_from(&later, &pb, None).unwrap();
        let rows: i64 = a
            .read()
            .query_row("SELECT COUNT(*) FROM outside_revenue", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
        let got = outside_revenue(a.read(), business, 2026).unwrap().unwrap();
        assert_eq!(got.amount, Cents(30_000));
    }

    #[test]
    fn deleting_business_removes_outside_revenue() {
        let mut w = small_world();
        w.db.write(|tx| set_outside_revenue(tx, &clock(), w.business, 2026, Cents(10_000), ""))
            .unwrap();
        w.db.write(|tx| crate::masterdata::delete_business(tx, &clock(), w.business))
            .unwrap();
        let rows: i64 =
            w.db.read()
                .query_row("SELECT COUNT(*) FROM outside_revenue", [], |r| r.get(0))
                .unwrap();
        assert_eq!(rows, 0);
        let marked: i64 =
            w.db.read()
                .query_row(
                    "SELECT COUNT(*) FROM tombstones WHERE table_name = 'outside_revenue'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
        assert_eq!(marked, 1);
    }

    #[test]
    fn outside_revenue_refuses_negative() {
        let mut w = small_world();
        let err =
            w.db.write(|tx| set_outside_revenue(tx, &clock(), w.business, 2026, Cents(-1), ""))
                .unwrap_err();
        assert!(matches!(err, crate::Error::Validation(_)), "{err:?}");
    }

    #[test]
    fn tiles_split_by_currency() {
        let mut w = world_with(sample_business());
        // Due on 2026-11-24, so not overdue yet.
        let eur = issue(&mut w, d(2026, 11, 10), dec!(100));
        pay(&mut w, eur, 1_000, d(2026, 11, 12));
        let (usd_business, usd_brand) =
            w.db.write(|tx| {
                create_business(
                    tx,
                    &clock(),
                    NewBusiness {
                        name: "Muster Studio".into(),
                        currency: "USD".into(),
                        ..sample_business()
                    },
                )
            })
            .unwrap();
        let input = draft_input(&w, usd_brand, d(2026, 10, 1), dec!(50));
        issue_with(&mut w, input);
        let o = overview(w.db.read(), None, today()).unwrap();
        let currencies: Vec<&str> = o.tiles.iter().map(|t| t.currency.as_str()).collect();
        assert_eq!(currencies, ["EUR", "USD"]);
        let eur_tiles = &o.tiles[0];
        assert_eq!(eur_tiles.open, Cents(11_900 - 1_000));
        assert_eq!(eur_tiles.open_count, 1);
        assert_eq!(eur_tiles.overdue_count, 0);
        assert_eq!(eur_tiles.received_this_month, Cents(1_000));
        let usd_tiles = &o.tiles[1];
        assert_eq!(usd_tiles.open, Cents(5_950));
        assert_eq!(usd_tiles.overdue, Cents(5_950));
        assert_eq!(usd_tiles.overdue_count, 1);
        let only_usd = overview(w.db.read(), Some(usd_business), today()).unwrap();
        assert_eq!(only_usd.tiles.len(), 1);
        assert_eq!(only_usd.tiles[0].currency, "USD");
    }

    #[test]
    fn debtors_sorted_by_due_date_with_days_overdue() {
        let mut w = world_with(sample_business());
        let late = issue(&mut w, d(2026, 10, 1), dec!(100));
        let fresh = issue(&mut w, d(2026, 11, 18), dec!(10));
        let paid = issue(&mut w, d(2026, 9, 1), dec!(10));
        pay(&mut w, paid, 1_190, d(2026, 9, 2));
        let cancelled = issue(&mut w, d(2026, 8, 1), dec!(10));
        w.db.write(|tx| crate::storno::cancel_invoice(tx, &clock(), cancelled))
            .unwrap();
        let draft = draft_input(&w, w.brand, d(2026, 7, 1), dec!(10));
        w.db.write(|tx| create_draft(tx, &clock(), draft)).unwrap();
        // The list shows the name that was printed, not a later one.
        let mut customer = get_customer(w.db.read(), w.customer).unwrap();
        customer.name = "Neuer Name".into();
        w.db.write(|tx| update_customer(tx, &clock(), &customer))
            .unwrap();
        let o = overview(w.db.read(), None, today()).unwrap();
        let ids: Vec<Uuid> = o.debtors.iter().map(|x| x.invoice_id).collect();
        assert_eq!(ids, [late, fresh]);
        assert_eq!(o.debtors[0].days_overdue, 34);
        assert_eq!(o.debtors[0].open, Cents(11_900));
        assert_eq!(o.debtors[0].customer_name, "Kunde A");
        assert_eq!(o.debtors[0].currency, "EUR");
        assert_eq!(o.debtors[1].days_overdue, -14);
    }

    #[test]
    fn monthly_receipts_has_twelve_months() {
        let mut w = world_with(sample_business());
        let a = issue(&mut w, d(2026, 3, 1), dec!(100));
        pay(&mut w, a, 5_000, d(2026, 3, 10));
        pay(&mut w, a, 1_000, d(2026, 11, 2));
        let old = issue(&mut w, d(2025, 6, 1), dec!(100));
        pay(&mut w, old, 700, d(2025, 6, 2));
        let o = overview(w.db.read(), None, today()).unwrap();
        assert_eq!(o.year, 2026);
        assert_eq!(o.monthly.len(), 1);
        let months = &o.monthly[0].months;
        assert_eq!(months.len(), 12);
        assert_eq!(months[2], Cents(5_000));
        assert_eq!(months[10], Cents(1_000));
        assert_eq!(months.iter().map(|c| c.0).sum::<i64>(), 6_000);
    }
}
