//! Invoices and their drafts.

use chrono::NaiveDate;
use rusqlite::{Connection, Row, Transaction, params};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::calc::{LineInput, LineKind, PriceMode, Totals, calculate};
use crate::dates::{Clock, ServiceDate, due_date, timestamp};
use crate::db::{
    UnknownValue, dec_col, json_col, not_found, opt_uuid_col, price_mode_col, uuid_col,
};
use crate::masterdata::{get_brand, get_business};
use crate::money::Cents;
use crate::tax::TaxChoice;
use crate::{Error, Result};

/// One line of an invoice as entered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineDraft {
    pub kind: LineKind,
    pub product_id: Option<Uuid>,
    pub description: String,
    pub quantity: Decimal,
    pub unit: String,
    pub unit_price: Decimal,
    pub tax: TaxChoice,
}

impl LineDraft {
    pub fn to_input(&self) -> LineInput {
        LineInput {
            kind: self.kind,
            quantity: self.quantity,
            unit_price: self.unit_price,
            tax: self.tax.clone(),
        }
    }
}

/// Everything that can be edited in a draft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftInput {
    pub brand_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub invoice_date: NaiveDate,
    pub service_date: ServiceDate,
    pub payment_days: u32,
    pub price_mode: PriceMode,
    pub language: String,
    pub intro_text: String,
    pub outro_text: String,
    pub notes: String,
    pub lines: Vec<LineDraft>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceState {
    Draft,
    Issued,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceKind {
    Invoice,
    Storno,
}

/// A stored invoice (draft or issued).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Invoice {
    pub id: Uuid,
    pub business_id: Uuid,
    pub brand_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub kind: InvoiceKind,
    pub state: InvoiceState,
    pub number: Option<String>,
    pub invoice_date: NaiveDate,
    pub service_date: ServiceDate,
    pub payment_days: u32,
    pub due_date: NaiveDate,
    pub price_mode: PriceMode,
    pub currency: String,
    pub language: String,
    pub intro_text: String,
    pub outro_text: String,
    pub notes: String,
    pub lines: Vec<LineDraft>,
    /// Totals frozen at issue; `None` for drafts.
    pub net: Option<Cents>,
    pub tax: Option<Cents>,
    pub gross: Option<Cents>,
    pub cancels_id: Option<Uuid>,
    pub cancelled_by_id: Option<Uuid>,
    pub replaces_id: Option<Uuid>,
    pub imported: bool,
    pub needs_review: bool,
    /// Taken over from VibeBill as cancelled, without a cancellation document.
    pub legacy_cancelled: bool,
}

const INVOICE_COLS: &str = "id, business_id, brand_id, customer_id, kind, state, number, \
    invoice_date, service_date, payment_days, due_date, price_mode, currency, language, \
    intro_text, outro_text, notes, net_cents, tax_cents, gross_cents, cancels_id, \
    cancelled_by_id, replaces_id, imported, needs_review, legacy_cancelled";

fn invoice_from_row(r: &Row) -> rusqlite::Result<Invoice> {
    let kind: String = r.get(4)?;
    let state: String = r.get(5)?;
    let bad = |index: usize, text: &str| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(UnknownValue(text.to_string())),
        )
    };
    Ok(Invoice {
        id: uuid_col(r, 0)?,
        business_id: uuid_col(r, 1)?,
        brand_id: uuid_col(r, 2)?,
        customer_id: opt_uuid_col(r, 3)?,
        kind: match kind.as_str() {
            "invoice" => InvoiceKind::Invoice,
            "storno" => InvoiceKind::Storno,
            other => return Err(bad(4, other)),
        },
        state: match state.as_str() {
            "draft" => InvoiceState::Draft,
            "issued" => InvoiceState::Issued,
            other => return Err(bad(5, other)),
        },
        number: r.get(6)?,
        invoice_date: r.get(7)?,
        service_date: json_col(r, 8)?,
        payment_days: r.get(9)?,
        due_date: r.get(10)?,
        price_mode: price_mode_col(r, 11)?,
        currency: r.get(12)?,
        language: r.get(13)?,
        intro_text: r.get(14)?,
        outro_text: r.get(15)?,
        notes: r.get(16)?,
        lines: Vec::new(),
        net: r.get::<_, Option<i64>>(17)?.map(Cents),
        tax: r.get::<_, Option<i64>>(18)?.map(Cents),
        gross: r.get::<_, Option<i64>>(19)?.map(Cents),
        cancels_id: opt_uuid_col(r, 20)?,
        cancelled_by_id: opt_uuid_col(r, 21)?,
        replaces_id: opt_uuid_col(r, 22)?,
        imported: r.get(23)?,
        needs_review: r.get(24)?,
        legacy_cancelled: r.get(25)?,
    })
}

fn line_from_row(r: &Row) -> rusqlite::Result<LineDraft> {
    let kind: String = r.get(0)?;
    let kind = match kind.as_str() {
        "item" => LineKind::Item,
        "free" => LineKind::Free,
        "shipping" => LineKind::Shipping,
        "discount" => LineKind::Discount,
        other => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(UnknownValue(other.to_string())),
            ));
        }
    };
    Ok(LineDraft {
        kind,
        product_id: opt_uuid_col(r, 1)?,
        description: r.get(2)?,
        quantity: dec_col(r, 3)?,
        unit: r.get(4)?,
        unit_price: dec_col(r, 5)?,
        tax: json_col(r, 6)?,
    })
}

pub(crate) fn line_kind_str(kind: LineKind) -> &'static str {
    match kind {
        LineKind::Item => "item",
        LineKind::Free => "free",
        LineKind::Shipping => "shipping",
        LineKind::Discount => "discount",
    }
}

/// Writes the lines of an invoice, numbered from 1.
pub(crate) fn insert_lines(tx: &Transaction, invoice_id: Uuid, lines: &[LineDraft]) -> Result<()> {
    for (index, line) in lines.iter().enumerate() {
        tx.execute(
            "INSERT INTO invoice_lines (id, invoice_id, position, kind, product_id, description,
                quantity, unit, unit_price, tax)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                Uuid::new_v4().to_string(),
                invoice_id.to_string(),
                index as i64 + 1,
                line_kind_str(line.kind),
                line.product_id.map(|p| p.to_string()),
                line.description,
                line.quantity.to_string(),
                line.unit,
                line.unit_price.to_string(),
                serde_json::to_string(&line.tax)?,
            ],
        )?;
    }
    Ok(())
}

/// Creates a draft. Business and currency come from the brand.
pub fn create_draft(tx: &Transaction, clock: &dyn Clock, input: DraftInput) -> Result<Uuid> {
    let id = Uuid::new_v4();
    let brand = get_brand(tx, input.brand_id)?;
    let business = get_business(tx, brand.business_id)?;
    let now = timestamp(clock);
    tx.execute(
        "INSERT INTO invoices (id, business_id, brand_id, customer_id, kind, state, invoice_date,
            service_date, payment_days, due_date, price_mode, currency, language, intro_text,
            outro_text, notes, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'invoice', 'draft', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?15)",
        params![
            id.to_string(),
            business.id.to_string(),
            brand.id.to_string(),
            input.customer_id.map(|c| c.to_string()),
            input.invoice_date,
            serde_json::to_string(&input.service_date)?,
            input.payment_days,
            due_date(input.invoice_date, input.payment_days),
            input.price_mode.as_str(),
            business.currency,
            input.language,
            input.intro_text,
            input.outro_text,
            input.notes,
            now,
        ],
    )?;
    insert_lines(tx, id, &input.lines)?;
    Ok(id)
}

/// Saves a draft, replacing all its lines.
pub fn save_draft(tx: &Transaction, clock: &dyn Clock, id: Uuid, input: DraftInput) -> Result<()> {
    require_draft(tx, id)?;
    let brand = get_brand(tx, input.brand_id)?;
    let business = get_business(tx, brand.business_id)?;
    tx.execute(
        "UPDATE invoices SET business_id = ?2, brand_id = ?3, customer_id = ?4, invoice_date = ?5,
            service_date = ?6, payment_days = ?7, due_date = ?8, price_mode = ?9, currency = ?10,
            language = ?11, intro_text = ?12, outro_text = ?13, notes = ?14, updated_at = ?15
         WHERE id = ?1",
        params![
            id.to_string(),
            business.id.to_string(),
            brand.id.to_string(),
            input.customer_id.map(|c| c.to_string()),
            input.invoice_date,
            serde_json::to_string(&input.service_date)?,
            input.payment_days,
            due_date(input.invoice_date, input.payment_days),
            input.price_mode.as_str(),
            business.currency,
            input.language,
            input.intro_text,
            input.outro_text,
            input.notes,
            timestamp(clock),
        ],
    )?;
    tx.execute(
        "DELETE FROM invoice_lines WHERE invoice_id = ?1",
        [id.to_string()],
    )?;
    insert_lines(tx, id, &input.lines)
}

/// Deletes a draft. Issued invoices can never be deleted.
pub fn delete_draft(tx: &Transaction, clock: &dyn Clock, id: Uuid) -> Result<()> {
    require_draft(tx, id)?;
    tx.execute("DELETE FROM invoices WHERE id = ?1", [id.to_string()])?;
    crate::tombstone::mark_deleted(tx, clock, "invoices", id)
}

fn require_draft(conn: &Connection, id: Uuid) -> Result<()> {
    let state: String = conn
        .query_row(
            "SELECT state FROM invoices WHERE id = ?1",
            [id.to_string()],
            |r| r.get(0),
        )
        .map_err(not_found)?;
    if state != "draft" {
        return Err(Error::NotDraft);
    }
    Ok(())
}

/// Loads an invoice with its lines.
pub fn load_invoice(conn: &Connection, id: Uuid) -> Result<Invoice> {
    let mut invoice = conn
        .query_row(
            &format!("SELECT {INVOICE_COLS} FROM invoices WHERE id = ?1"),
            [id.to_string()],
            invoice_from_row,
        )
        .map_err(not_found)?;
    let mut stmt = conn.prepare(
        "SELECT kind, product_id, description, quantity, unit, unit_price, tax
         FROM invoice_lines WHERE invoice_id = ?1 ORDER BY position",
    )?;
    invoice.lines = stmt
        .query_map([id.to_string()], line_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(invoice)
}

/// Calculates the totals of an invoice from its lines.
pub fn draft_totals(conn: &Connection, id: Uuid) -> Result<Totals> {
    let invoice = load_invoice(conn, id)?;
    let business = get_business(conn, invoice.business_id)?;
    let lines: Vec<LineInput> = invoice.lines.iter().map(LineDraft::to_input).collect();
    Ok(calculate(
        &lines,
        invoice.price_mode,
        business.small_business,
    )?)
}

/// Copies an invoice into a new draft dated today, without number or
/// payments.
pub fn duplicate_as_draft(tx: &Transaction, clock: &dyn Clock, id: Uuid) -> Result<Uuid> {
    let source = load_invoice(tx, id)?;
    create_draft(
        tx,
        clock,
        DraftInput {
            brand_id: source.brand_id,
            customer_id: source.customer_id,
            invoice_date: clock.today(),
            service_date: ServiceDate::SameAsInvoice,
            payment_days: source.payment_days,
            price_mode: source.price_mode,
            language: source.language,
            intro_text: source.intro_text,
            outro_text: source.outro_text,
            notes: source.notes,
            lines: source.lines,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::FixedClock;
    use crate::db::Db;
    use crate::masterdata::{NewBusiness, create_business, create_customer};
    use crate::money::{Cents, parse_decimal};
    use crate::testutil::{clock, d, sample_business, sample_customer, sample_draft};

    fn setup(business: NewBusiness) -> (Db, Uuid, Uuid) {
        let mut db = Db::open_in_memory().unwrap();
        let (_, brand_id) = db
            .write(|tx| create_business(tx, &clock(), business))
            .unwrap();
        let customer_id = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        (db, brand_id, customer_id)
    }

    #[test]
    fn draft_roundtrip() {
        let (mut db, brand_id, customer_id) = setup(sample_business());
        let mut input = sample_draft(brand_id, Some(customer_id));
        input.lines[0].quantity = parse_decimal("1,5").unwrap();
        input.service_date = ServiceDate::Period {
            from: d(2026, 11, 1),
            to: d(2026, 11, 15),
        };
        let id = db
            .write(|tx| create_draft(tx, &clock(), input.clone()))
            .unwrap();
        let invoice = load_invoice(db.read(), id).unwrap();
        assert_eq!(invoice.state, InvoiceState::Draft);
        assert_eq!(invoice.kind, InvoiceKind::Invoice);
        assert_eq!(invoice.number, None);
        assert_eq!(invoice.brand_id, brand_id);
        assert_eq!(invoice.customer_id, Some(customer_id));
        assert_eq!(invoice.currency, "EUR");
        assert_eq!(invoice.service_date, input.service_date);
        assert_eq!(invoice.lines, input.lines);
    }

    #[test]
    fn draft_due_date_from_payment_days() {
        let (mut db, brand_id, customer_id) = setup(sample_business());
        let id = db
            .write(|tx| create_draft(tx, &clock(), sample_draft(brand_id, Some(customer_id))))
            .unwrap();
        let invoice = load_invoice(db.read(), id).unwrap();
        assert_eq!(invoice.invoice_date, d(2026, 11, 18));
        assert_eq!(invoice.due_date, d(2026, 12, 2));
    }

    #[test]
    fn save_draft_replaces_lines() {
        let (mut db, brand_id, customer_id) = setup(sample_business());
        let id = db
            .write(|tx| create_draft(tx, &clock(), sample_draft(brand_id, Some(customer_id))))
            .unwrap();
        let mut input = sample_draft(brand_id, Some(customer_id));
        input.lines.push(LineDraft {
            description: "Zweite Position".into(),
            ..input.lines[0].clone()
        });
        input.payment_days = 30;
        db.write(|tx| save_draft(tx, &clock(), id, input.clone()))
            .unwrap();
        let invoice = load_invoice(db.read(), id).unwrap();
        assert_eq!(invoice.lines.len(), 2);
        assert_eq!(invoice.lines[1].description, "Zweite Position");
        assert_eq!(invoice.due_date, d(2026, 12, 18));
    }

    #[test]
    fn save_issued_is_refused() {
        let (mut db, brand_id, customer_id) = setup(sample_business());
        let id = db
            .write(|tx| create_draft(tx, &clock(), sample_draft(brand_id, Some(customer_id))))
            .unwrap();
        db.read()
            .execute(
                "UPDATE invoices SET state = 'issued', number = 'X-1' WHERE id = ?1",
                [id.to_string()],
            )
            .unwrap();
        let result =
            db.write(|tx| save_draft(tx, &clock(), id, sample_draft(brand_id, Some(customer_id))));
        assert!(matches!(result, Err(Error::NotDraft)));
        assert!(matches!(
            db.write(|tx| delete_draft(tx, &clock(), id)),
            Err(Error::NotDraft)
        ));
    }

    #[test]
    fn delete_draft_removes_it() {
        let (mut db, brand_id, customer_id) = setup(sample_business());
        let id = db
            .write(|tx| create_draft(tx, &clock(), sample_draft(brand_id, Some(customer_id))))
            .unwrap();
        db.write(|tx| delete_draft(tx, &clock(), id)).unwrap();
        assert!(matches!(load_invoice(db.read(), id), Err(Error::NotFound)));
    }

    #[test]
    fn draft_totals_regular_and_small_business() {
        let (mut db, brand_id, customer_id) = setup(sample_business());
        let id = db
            .write(|tx| create_draft(tx, &clock(), sample_draft(brand_id, Some(customer_id))))
            .unwrap();
        let totals = draft_totals(db.read(), id).unwrap();
        assert_eq!(
            (totals.net, totals.tax, totals.gross),
            (Cents(2500), Cents(475), Cents(2975))
        );

        let small = NewBusiness {
            small_business: true,
            ..sample_business()
        };
        let (mut db, brand_id, customer_id) = setup(small);
        let id = db
            .write(|tx| create_draft(tx, &clock(), sample_draft(brand_id, Some(customer_id))))
            .unwrap();
        let totals = draft_totals(db.read(), id).unwrap();
        assert_eq!((totals.tax, totals.gross), (Cents(0), Cents(2500)));
    }

    #[test]
    fn duplicate_resets_date_and_number() {
        let (mut db, brand_id, customer_id) = setup(sample_business());
        let mut input = sample_draft(brand_id, Some(customer_id));
        input.invoice_date = d(2026, 10, 1);
        input.service_date = ServiceDate::Day {
            date: d(2026, 9, 30),
        };
        let id = db
            .write(|tx| create_draft(tx, &clock(), input.clone()))
            .unwrap();
        let later = FixedClock {
            today: d(2026, 11, 18),
        };
        let copy = db.write(|tx| duplicate_as_draft(tx, &later, id)).unwrap();
        let invoice = load_invoice(db.read(), copy).unwrap();
        assert_ne!(copy, id);
        assert_eq!(invoice.invoice_date, d(2026, 11, 18));
        assert_eq!(invoice.due_date, d(2026, 12, 2));
        assert_eq!(invoice.service_date, ServiceDate::SameAsInvoice);
        assert_eq!(invoice.number, None);
        assert_eq!(invoice.replaces_id, None);
        assert_eq!(invoice.lines, input.lines);
    }
}
