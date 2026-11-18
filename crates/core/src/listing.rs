//! The invoice list as the interface shows it: one row per invoice with
//! its customer, amount and payment status, filtered in the core.

use chrono::NaiveDate;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Result;
use crate::invoice::{InvoiceKind, InvoiceState};
use crate::money::Cents;
use crate::payments::{InvoiceStatus, StatusKind};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct InvoiceFilter {
    pub business_id: Option<Uuid>,
    pub status: Option<StatusKind>,
    pub year: Option<i32>,
    /// Matches number and customer name, ignoring case.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct InvoiceRow {
    pub id: Uuid,
    pub number: Option<String>,
    pub kind: InvoiceKind,
    pub state: InvoiceState,
    pub business_id: Uuid,
    pub brand_id: Uuid,
    pub customer_name: Option<String>,
    pub invoice_date: NaiveDate,
    pub due_date: NaiveDate,
    pub gross: Cents,
    pub currency: String,
    pub status: InvoiceStatus,
}

/// The invoices matching `filter`, newest first. Drafts show their current
/// totals and customer, issued documents the frozen totals and the customer
/// name as printed.
pub fn list_invoices(
    conn: &Connection,
    filter: &InvoiceFilter,
    today: NaiveDate,
) -> Result<Vec<InvoiceRow>> {
    use chrono::Datelike;
    let mut stmt = conn.prepare(
        "SELECT i.id, c.name FROM invoices i LEFT JOIN customers c ON c.id = i.customer_id
         ORDER BY i.invoice_date DESC, i.number DESC, i.created_at DESC",
    )?;
    let rows: Vec<(Uuid, Option<String>)> = stmt
        .query_map([], |r| Ok((crate::db::uuid_col(r, 0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let text = filter.text.trim().to_lowercase();
    let mut out = Vec::new();
    for (id, current_name) in rows {
        let invoice = crate::invoice::load_invoice(conn, id)?;
        let customer_name = match invoice.state {
            InvoiceState::Draft => current_name,
            InvoiceState::Issued => {
                crate::overview::printed_customer_name(conn, id)?.or(current_name)
            }
        };
        if filter.business_id.is_some_and(|b| b != invoice.business_id)
            || filter
                .year
                .is_some_and(|y| y != invoice.invoice_date.year())
        {
            continue;
        }
        if !text.is_empty() {
            let hit = |s: Option<&str>| s.is_some_and(|s| s.to_lowercase().contains(&text));
            if !hit(invoice.number.as_deref()) && !hit(customer_name.as_deref()) {
                continue;
            }
        }
        let status = crate::payments::invoice_status(conn, id, today)?;
        if filter.status.is_some_and(|s| s != status.kind) {
            continue;
        }
        let gross = match invoice.state {
            InvoiceState::Draft => crate::invoice::draft_totals(conn, id)?.gross,
            InvoiceState::Issued => invoice.gross.unwrap_or(Cents::ZERO),
        };
        out.push(InvoiceRow {
            id,
            number: invoice.number,
            kind: invoice.kind,
            state: invoice.state,
            business_id: invoice.business_id,
            brand_id: invoice.brand_id,
            customer_name,
            invoice_date: invoice.invoice_date,
            due_date: invoice.due_date,
            gross,
            currency: invoice.currency,
            status,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::invoice::create_draft;
    use crate::issue::{IssueOptions, issue_invoice};
    use crate::masterdata::{NewBusiness, NewCustomer, create_business, create_customer};
    use crate::testutil::{clock, d, sample_business, sample_customer, sample_draft};

    struct World {
        db: Db,
        a: (Uuid, Uuid),
        b: (Uuid, Uuid),
        kunde: Uuid,
        club: Uuid,
    }

    fn world() -> World {
        let mut db = Db::open_in_memory().unwrap();
        let a = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let b = db
            .write(|tx| {
                create_business(
                    tx,
                    &clock(),
                    NewBusiness {
                        name: "Muster Studio".into(),
                        ..sample_business()
                    },
                )
            })
            .unwrap();
        let kunde = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        let club = db
            .write(|tx| {
                create_customer(
                    tx,
                    &clock(),
                    NewCustomer {
                        name: "Club Beispiel".into(),
                        ..sample_customer()
                    },
                )
            })
            .unwrap();
        World {
            db,
            a,
            b,
            kunde,
            club,
        }
    }

    fn draft_on(w: &mut World, brand: Uuid, customer: Uuid, date: NaiveDate) -> Uuid {
        let mut input = sample_draft(brand, Some(customer));
        input.invoice_date = date;
        w.db.write(|tx| create_draft(tx, &clock(), input)).unwrap()
    }

    fn issue(w: &mut World, id: Uuid) {
        w.db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()))
            .unwrap();
    }

    #[test]
    fn list_shows_printed_customer_name_and_currency() {
        let mut w = world();
        let (brand, kunde, club) = (w.a.1, w.kunde, w.club);
        let issued = draft_on(&mut w, brand, kunde, d(2026, 11, 1));
        issue(&mut w, issued);
        let draft = draft_on(&mut w, brand, kunde, d(2026, 11, 2));
        let mut customer = crate::masterdata::get_customer(w.db.read(), w.kunde).unwrap();
        customer.name = "Neuer Name".into();
        w.db.write(|tx| crate::masterdata::update_customer(tx, &clock(), &customer))
            .unwrap();
        let (_, usd_brand) =
            w.db.write(|tx| {
                create_business(
                    tx,
                    &clock(),
                    NewBusiness {
                        name: "Dollar Studio".into(),
                        currency: "USD".into(),
                        ..sample_business()
                    },
                )
            })
            .unwrap();
        let usd = draft_on(&mut w, usd_brand, club, d(2026, 11, 3));
        let rows = list_invoices(w.db.read(), &InvoiceFilter::default(), d(2026, 11, 18)).unwrap();
        let row = |id: Uuid| rows.iter().find(|r| r.id == id).unwrap();
        assert_eq!(row(issued).customer_name.as_deref(), Some("Kunde A"));
        assert_eq!(row(draft).customer_name.as_deref(), Some("Neuer Name"));
        assert_eq!(row(issued).currency, "EUR");
        assert_eq!(row(usd).currency, "USD");
    }

    fn ids(w: &World, filter: InvoiceFilter) -> Vec<Uuid> {
        list_invoices(w.db.read(), &filter, d(2026, 11, 18))
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect()
    }

    #[test]
    fn list_filters_by_business_status_year_and_text() {
        let mut w = world();
        let (brand_a, brand_b, kunde, club) = (w.a.1, w.b.1, w.kunde, w.club);
        let one = draft_on(&mut w, brand_a, kunde, d(2026, 11, 18));
        issue(&mut w, one);
        let two = draft_on(&mut w, brand_b, club, d(2025, 3, 1));
        let all = InvoiceFilter::default();
        assert_eq!(ids(&w, all.clone()), vec![one, two]);
        let business_a = InvoiceFilter {
            business_id: Some(w.a.0),
            ..all.clone()
        };
        assert_eq!(ids(&w, business_a), vec![one]);
        let drafts = InvoiceFilter {
            status: Some(StatusKind::Draft),
            ..all.clone()
        };
        assert_eq!(ids(&w, drafts), vec![two]);
        let year = InvoiceFilter {
            year: Some(2025),
            ..all.clone()
        };
        assert_eq!(ids(&w, year), vec![two]);
        let club_text = InvoiceFilter {
            text: "CLUB".into(),
            ..all.clone()
        };
        assert_eq!(ids(&w, club_text), vec![two]);
        let number = InvoiceFilter {
            text: "re-2026-0001".into(),
            ..all
        };
        assert_eq!(ids(&w, number), vec![one]);
    }

    #[test]
    fn list_shows_draft_totals_and_frozen_totals() {
        let mut w = world();
        let (brand, kunde) = (w.a.1, w.kunde);
        let issued = draft_on(&mut w, brand, kunde, d(2026, 11, 18));
        issue(&mut w, issued);
        let draft = draft_on(&mut w, brand, kunde, d(2026, 11, 17));
        let rows = list_invoices(w.db.read(), &InvoiceFilter::default(), d(2026, 11, 18)).unwrap();
        let row = |id| rows.iter().find(|r| r.id == id).unwrap();
        assert_eq!(row(issued).gross, Cents(2975));
        assert_eq!(row(issued).number.as_deref(), Some("RE-2026-0001"));
        assert_eq!(row(issued).status.kind, StatusKind::Open);
        assert_eq!(row(issued).customer_name.as_deref(), Some("Kunde A"));
        assert_eq!(row(draft).gross, Cents(2975));
        assert_eq!(row(draft).number, None);
        assert_eq!(row(draft).status.kind, StatusKind::Draft);
    }

    #[test]
    fn peek_number_matches_issued_number() {
        let mut w = world();
        let (brand, kunde) = (w.a.1, w.kunde);
        let first = draft_on(&mut w, brand, kunde, d(2026, 11, 18));
        let second = draft_on(&mut w, brand, kunde, d(2026, 11, 18));
        let peeked = crate::numbering::peek_number(w.db.read(), first).unwrap();
        assert_eq!(peeked, "RE-2026-0001");
        assert_eq!(
            crate::numbering::peek_number(w.db.read(), second).unwrap(),
            "RE-2026-0001",
            "peeking twice does not count"
        );
        issue(&mut w, first);
        let number = crate::invoice::load_invoice(w.db.read(), first)
            .unwrap()
            .number;
        assert_eq!(number.as_deref(), Some(peeked.as_str()));
        assert_eq!(
            crate::numbering::peek_number(w.db.read(), second).unwrap(),
            "RE-2026-0002"
        );
    }

    #[test]
    fn list_sorts_newest_first() {
        let mut w = world();
        let (brand, kunde) = (w.a.1, w.kunde);
        let jan = draft_on(&mut w, brand, kunde, d(2026, 1, 1));
        let jun = draft_on(&mut w, brand, kunde, d(2026, 6, 1));
        let mar = draft_on(&mut w, brand, kunde, d(2026, 3, 1));
        assert_eq!(ids(&w, InvoiceFilter::default()), vec![jun, mar, jan]);
    }
}
