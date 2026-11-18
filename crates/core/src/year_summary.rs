//! The year summary of one business: per month what was invoiced (stornos
//! subtracted) and what came in, and what was still open at the end.

use chrono::{Datelike, NaiveDate};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Result;
use crate::money::Cents;
use crate::overview::year_bounds;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct YearMonth {
    /// 1 to 12; 0 for the year's total.
    pub month: u32,
    /// Invoices issued (cancellation documents are not counted).
    pub count: u32,
    pub net: Cents,
    pub tax: Cents,
    pub gross: Cents,
    pub received: Cents,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct YearSummary {
    pub business_id: Uuid,
    pub business_name: String,
    pub year: i32,
    pub currency: String,
    pub months: Vec<YearMonth>,
    pub total: YearMonth,
    /// The day "open" refers to: 31 December, or today in the current year.
    pub open_at: NaiveDate,
    pub open_at_end: Cents,
    pub open_count: u32,
    /// The day the summary was made, printed in the PDF footer.
    pub created_on: NaiveDate,
}

/// The summary of `business_id` for `year`, as known on `today`.
pub fn year_summary(
    conn: &Connection,
    business_id: Uuid,
    year: i32,
    today: NaiveDate,
) -> Result<YearSummary> {
    let business = crate::masterdata::get_business(conn, business_id)?;
    let (from, to) = year_bounds(year);
    let id = business_id.to_string();
    let mut months: Vec<YearMonth> = (1..=12)
        .map(|month| YearMonth {
            month,
            ..YearMonth::default()
        })
        .collect();
    // Issued documents by the month of their date; cancellation documents
    // carry negative totals and so subtract. Invoices VibeBill cancelled
    // without a document do not count.
    let mut stmt = conn.prepare(
        "SELECT CAST(strftime('%m', invoice_date) AS INTEGER), kind,
                COALESCE(net_cents, 0), COALESCE(tax_cents, 0), COALESCE(gross_cents, 0)
         FROM invoices
         WHERE business_id = ?1 AND state = 'issued' AND legacy_cancelled = 0
           AND invoice_date BETWEEN ?2 AND ?3",
    )?;
    let docs: Vec<(u32, String, i64, i64, i64)> = stmt
        .query_map(params![id, from, to], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    for (month, kind, net, tax, gross) in docs {
        let m = &mut months[month as usize - 1];
        if kind == "invoice" {
            m.count += 1;
        }
        m.net = m.net.checked_add(Cents(net))?;
        m.tax = m.tax.checked_add(Cents(tax))?;
        m.gross = m.gross.checked_add(Cents(gross))?;
    }
    let mut stmt = conn.prepare(
        "SELECT CAST(strftime('%m', p.date) AS INTEGER), SUM(p.amount_cents)
         FROM payments p JOIN invoices i ON i.id = p.invoice_id
         WHERE i.business_id = ?1 AND p.transfer_of_id IS NULL AND p.date BETWEEN ?2 AND ?3
         GROUP BY 1",
    )?;
    let received: Vec<(u32, i64)> = stmt
        .query_map(params![id, from, to], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (month, cents) in received {
        months[month as usize - 1].received = Cents(cents);
    }
    let mut total = YearMonth::default();
    for m in &months {
        total.count += m.count;
        total.net = total.net.checked_add(m.net)?;
        total.tax = total.tax.checked_add(m.tax)?;
        total.gross = total.gross.checked_add(m.gross)?;
        total.received = total.received.checked_add(m.received)?;
    }
    let open_at = if year == today.year() {
        today.min(to)
    } else {
        to
    };
    let (open_at_end, open_count) = open_on(conn, business_id, open_at)?;
    Ok(YearSummary {
        business_id,
        business_name: business.name,
        year,
        currency: business.currency,
        months,
        total,
        open_at,
        open_at_end,
        open_count,
        created_on: today,
    })
}

/// What was owed on `day`: each invoice issued by then and not cancelled
/// by then, less its payments made by then.
fn open_on(conn: &Connection, business_id: Uuid, day: NaiveDate) -> Result<(Cents, u32)> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(i.gross_cents, 0) - COALESCE(
                    (SELECT SUM(p.amount_cents) FROM payments p
                     WHERE p.invoice_id = i.id AND p.date <= ?2), 0)
         FROM invoices i
         WHERE i.business_id = ?1 AND i.state = 'issued' AND i.kind = 'invoice'
           AND i.legacy_cancelled = 0 AND i.invoice_date <= ?2
           AND NOT EXISTS (SELECT 1 FROM invoices s
                           WHERE s.cancels_id = i.id AND s.state = 'issued'
                             AND s.invoice_date <= ?2)",
    )?;
    let open: Vec<i64> = stmt
        .query_map(params![business_id.to_string(), day], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut sum = Cents::ZERO;
    let mut count = 0;
    for cents in open.into_iter().filter(|c| *c > 0) {
        sum = sum.checked_add(Cents(cents))?;
        count += 1;
    }
    Ok((sum, count))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::dates::FixedClock;
    use crate::db::Db;
    use crate::invoice::create_draft;
    use crate::issue::{IssueOptions, issue_invoice};
    use crate::masterdata::{create_business, create_customer};
    use crate::payments::{PaymentMethod, add_payment};
    use crate::testutil::{clock, d, sample_business, sample_customer, sample_draft};
    use rust_decimal::{Decimal, dec};

    pub(crate) struct World {
        pub db: Db,
        pub business: Uuid,
        pub brand: Uuid,
        pub customer: Uuid,
    }

    pub(crate) fn world() -> World {
        let mut db = Db::open_in_memory().unwrap();
        let (business, brand) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
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

    /// Issues 1 × `net` at 19 % dated `date`.
    pub(crate) fn issue(w: &mut World, date: NaiveDate, net: Decimal) -> Uuid {
        let mut input = sample_draft(w.brand, Some(w.customer));
        input.invoice_date = date;
        input.lines[0].quantity = dec!(1);
        input.lines[0].unit_price = net;
        w.db.write(|tx| {
            let id = create_draft(tx, &clock(), input)?;
            issue_invoice(tx, &clock(), id, IssueOptions::default())?;
            Ok(id)
        })
        .unwrap()
    }

    pub(crate) fn pay(w: &mut World, id: Uuid, cents: i64, date: NaiveDate) {
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

    fn on(day: NaiveDate) -> FixedClock {
        FixedClock { today: day }
    }

    fn cancel_on(w: &mut World, id: Uuid, day: NaiveDate) {
        w.db.write(|tx| crate::storno::cancel_invoice(tx, &on(day), id))
            .unwrap();
    }

    fn summary(w: &World, year: i32) -> YearSummary {
        year_summary(w.db.read(), w.business, year, d(2026, 11, 18)).unwrap()
    }

    #[test]
    fn year_summary_subtracts_stornos_in_their_month() {
        let mut w = world();
        let a = issue(&mut w, d(2026, 3, 4), dec!(100));
        issue(&mut w, d(2026, 3, 20), dec!(10));
        cancel_on(&mut w, a, d(2026, 5, 10));
        let s = summary(&w, 2026);
        assert_eq!(s.months.len(), 12);
        assert_eq!(s.months[2].count, 2);
        assert_eq!(s.months[2].net, Cents(11_000));
        assert_eq!(s.months[2].tax, Cents(2_090));
        assert_eq!(s.months[2].gross, Cents(13_090));
        assert_eq!(s.months[4].count, 0);
        assert_eq!(s.months[4].gross, Cents(-11_900));
        assert_eq!(s.total.month, 0);
        assert_eq!(s.total.count, 2);
        assert_eq!(s.total.gross, Cents(1_190));
        assert_eq!(s.business_name, "Beispiel GbR");
        assert_eq!(s.currency, "EUR");
        assert_eq!(s.created_on, d(2026, 11, 18));
    }

    #[test]
    fn storno_next_year_counts_in_next_year() {
        let mut w = world();
        let a = issue(&mut w, d(2025, 12, 15), dec!(100));
        cancel_on(&mut w, a, d(2026, 1, 15));
        let last = summary(&w, 2025);
        assert_eq!(last.months[11].gross, Cents(11_900));
        assert_eq!(last.total.gross, Cents(11_900));
        let this = summary(&w, 2026);
        assert_eq!(this.months[0].gross, Cents(-11_900));
    }

    #[test]
    fn year_summary_counts_transferred_payment_once() {
        let mut w = world();
        let a = issue(&mut w, d(2026, 3, 4), dec!(100));
        pay(&mut w, a, 5_000, d(2026, 3, 10));
        let (_, replacement) =
            w.db.write(|tx| crate::storno::correct_invoice(tx, &on(d(2026, 4, 2)), a))
                .unwrap();
        w.db.write(|tx| {
            issue_invoice(tx, &on(d(2026, 4, 2)), replacement, IssueOptions::default())
        })
        .unwrap();
        let s = summary(&w, 2026);
        assert_eq!(s.months[2].received, Cents(5_000));
        assert_eq!(s.total.received, Cents(5_000));
        // Still owed on the replacement: 119,00 € − 50,00 €.
        assert_eq!(s.open_at_end, Cents(6_900));
        assert_eq!(s.open_count, 1);
    }

    #[test]
    fn open_at_year_end_counts_invoice_cancelled_next_year() {
        let mut w = world();
        let a = issue(&mut w, d(2025, 11, 3), dec!(100));
        cancel_on(&mut w, a, d(2026, 1, 10));
        let last = summary(&w, 2025);
        assert_eq!(last.open_at, d(2025, 12, 31));
        assert_eq!(last.open_at_end, Cents(11_900));
        assert_eq!(last.open_count, 1);
        let this = summary(&w, 2026);
        assert_eq!(this.open_at, d(2026, 11, 18));
        assert_eq!(this.open_at_end, Cents::ZERO);
        assert_eq!(this.open_count, 0);
    }

    #[test]
    fn open_at_year_end_ignores_later_payments() {
        let mut w = world();
        let a = issue(&mut w, d(2025, 12, 1), dec!(100));
        pay(&mut w, a, 4_000, d(2025, 12, 20));
        pay(&mut w, a, 7_900, d(2026, 1, 5));
        let last = summary(&w, 2025);
        assert_eq!(last.open_at_end, Cents(7_900));
        assert_eq!(last.total.received, Cents(4_000));
        let this = summary(&w, 2026);
        assert_eq!(this.open_at_end, Cents::ZERO);
        assert_eq!(this.months[0].received, Cents(7_900));
    }

    #[test]
    fn year_summary_skips_legacy_cancelled() {
        let mut w = world();
        issue(&mut w, d(2026, 11, 2), dec!(10));
        let raw = crate::testutil::insert_raw_invoice(
            w.db.read(),
            w.business,
            w.brand,
            Some(w.customer),
            "draft",
        );
        w.db.read()
            .execute(
                "UPDATE invoices SET state = 'issued', number = 'ALT-1', net_cents = 5000,
                    tax_cents = 950, gross_cents = 5950, imported = 1, legacy_cancelled = 1
                 WHERE id = ?1",
                [raw.to_string()],
            )
            .unwrap();
        let s = summary(&w, 2026);
        assert_eq!(s.months[10].count, 1);
        assert_eq!(s.months[10].gross, Cents(1_190));
        assert_eq!(s.open_at_end, Cents(1_190));
    }
}
