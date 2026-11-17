//! Shared fixtures for tests. Neutral placeholder data only.

use chrono::NaiveDate;
use rusqlite::{Connection, params};
use uuid::Uuid;

use rust_decimal::dec;

use crate::calc::{LineKind, PriceMode};
use crate::dates::{FixedClock, ServiceDate};
use crate::invoice::{DraftInput, LineDraft};
use crate::masterdata::{CustomerKind, NewBusiness, NewCustomer};
use crate::tax::{TaxCategory, TaxChoice};

/// A draft dated 2026-11-18 with one line: 2 × 12,50 € net at 19 %.
pub fn sample_draft(brand_id: Uuid, customer_id: Option<Uuid>) -> DraftInput {
    DraftInput {
        brand_id,
        customer_id,
        invoice_date: d(2026, 11, 18),
        service_date: ServiceDate::SameAsInvoice,
        payment_days: 14,
        price_mode: PriceMode::Net,
        language: "de".into(),
        intro_text: String::new(),
        outro_text: String::new(),
        notes: String::new(),
        lines: vec![LineDraft {
            kind: LineKind::Item,
            product_id: None,
            description: "Leistung".into(),
            quantity: dec!(2),
            unit: "Stk".into(),
            unit_price: dec!(12.50),
            tax: TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(19) }),
        }],
    }
}

pub fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

pub fn clock() -> FixedClock {
    FixedClock {
        today: d(2026, 11, 18),
    }
}

pub fn sample_business() -> NewBusiness {
    NewBusiness {
        name: "Beispiel GbR".into(),
        street: "Musterstraße 1".into(),
        zip: "12345".into(),
        city: "Musterstadt".into(),
        tax_number: "12/345/67890".into(),
        ..NewBusiness::default()
    }
}

pub fn sample_customer() -> NewCustomer {
    NewCustomer {
        name: "Kunde A".into(),
        street: "Hauptstraße 2".into(),
        zip: "54321".into(),
        city: "Beispielstadt".into(),
        kind: CustomerKind::Business,
        ..NewCustomer::default()
    }
}

/// Inserts an invoice row directly, bypassing the business rules.
pub fn insert_raw_invoice(
    conn: &Connection,
    business_id: Uuid,
    brand_id: Uuid,
    customer_id: Option<Uuid>,
    state: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    conn.execute(
        "INSERT INTO invoices (id, business_id, brand_id, customer_id, state, number,
            invoice_date, service_date, payment_days, due_date, price_mode, currency, language,
            created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, '2026-11-18', '{\"type\":\"same_as_invoice\"}', 14,
            '2026-12-02', 'net', 'EUR', 'de', 'x', 'x')",
        params![
            id.to_string(),
            business_id.to_string(),
            brand_id.to_string(),
            customer_id.map(|c| c.to_string()),
            state,
            if state == "issued" {
                Some(format!("RAW-{}", &id.to_string()[..8]))
            } else {
                None
            },
        ],
    )
    .unwrap();
    id
}

/// Inserts an invoice line directly, bypassing the business rules.
pub fn insert_raw_line(conn: &Connection, invoice_id: Uuid, product_id: Option<Uuid>) -> Uuid {
    let id = Uuid::new_v4();
    conn.execute(
        "INSERT INTO invoice_lines (id, invoice_id, position, kind, product_id, description,
            quantity, unit, unit_price, tax)
         VALUES (?1, ?2, 1, 'item', ?3, 'Artikel', '1', 'Stk', '10',
            '{\"mode\":\"fixed\",\"category\":{\"type\":\"standard\",\"rate\":\"19\"}}')",
        params![
            id.to_string(),
            invoice_id.to_string(),
            product_id.map(|p| p.to_string())
        ],
    )
    .unwrap();
    id
}
