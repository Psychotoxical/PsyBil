//! What the PDF prints, read from the frozen snapshot of an issued invoice
//! or built from a draft the same way.

use chrono::NaiveDate;
use rusqlite::Connection;
use serde::Deserialize;
use uuid::Uuid;

use crate::calc::{PriceMode, Totals};
use crate::dates::ServiceDate;
use crate::invoice::{InvoiceKind, LineDraft};
use crate::masterdata::{Brand, Business, Customer};
use crate::money::Cents;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct InvoiceDoc {
    pub number: Option<String>,
    pub kind: InvoiceKind,
    pub invoice_date: NaiveDate,
    pub service_date: ServiceDate,
    pub due_date: NaiveDate,
    pub currency: String,
    pub language: String,
    pub price_mode: PriceMode,
    pub template: String,
    pub business: Business,
    pub brand: Brand,
    pub customer: Option<Customer>,
    pub lines: Vec<LineDraft>,
    pub totals: Totals,
    pub intro: String,
    pub outro: String,
    /// Number and date of the invoice a cancellation refers to.
    pub cancels: Option<(String, NaiveDate)>,
    pub imported: bool,
    pub payment_received: bool,
    /// Paid on the corrected invoice and moved to this replacement.
    pub paid_before: Cents,
    pub logo_sha256: Option<String>,
    /// When the snapshot was frozen; `None` for drafts.
    pub issued_at: Option<String>,
}

#[derive(Deserialize)]
struct Texts {
    intro: String,
    outro: String,
}

#[derive(Deserialize)]
struct Cancels {
    number: String,
    invoice_date: NaiveDate,
}

/// The snapshot as `issue::build_snapshot` writes it.
#[derive(Deserialize)]
struct Snapshot {
    number: String,
    kind: InvoiceKind,
    invoice_date: NaiveDate,
    service_date: ServiceDate,
    due_date: NaiveDate,
    currency: String,
    language: String,
    price_mode: PriceMode,
    pdf_template: String,
    business: Business,
    brand: Brand,
    logo_sha256: Option<String>,
    customer: Option<Customer>,
    lines: Vec<LineDraft>,
    totals: Totals,
    texts: Texts,
    cancels: Option<Cancels>,
    #[serde(default)]
    imported: bool,
    #[serde(default)]
    payment_received: bool,
    #[serde(default)]
    paid_before: i64,
}

impl InvoiceDoc {
    /// Reads a snapshot. `created_at` is the snapshot's timestamp.
    pub fn from_snapshot(snapshot: &serde_json::Value, created_at: Option<&str>) -> Result<Self> {
        let s: Snapshot = serde_json::from_value(snapshot.clone())?;
        Ok(InvoiceDoc {
            number: Some(s.number).filter(|n| !n.is_empty()),
            kind: s.kind,
            invoice_date: s.invoice_date,
            service_date: s.service_date,
            due_date: s.due_date,
            currency: s.currency,
            language: s.language,
            price_mode: s.price_mode,
            template: s.pdf_template,
            business: s.business,
            brand: s.brand,
            customer: s.customer,
            lines: s.lines,
            totals: s.totals,
            intro: s.texts.intro,
            outro: s.texts.outro,
            cancels: s.cancels.map(|c| (c.number, c.invoice_date)),
            imported: s.imported,
            payment_received: s.payment_received,
            paid_before: Cents(s.paid_before),
            logo_sha256: s.logo_sha256,
            issued_at: created_at.map(str::to_string),
        })
    }
}

/// The document of a draft, with today's master data and totals.
pub fn draft_doc(conn: &Connection, id: Uuid) -> Result<InvoiceDoc> {
    let invoice = crate::invoice::load_invoice(conn, id)?;
    if invoice.state != crate::invoice::InvoiceState::Draft {
        return Err(Error::NotDraft);
    }
    let totals = crate::invoice::draft_totals(conn, id)?;
    let snapshot = crate::issue::build_snapshot(conn, &invoice, "", &totals, false)?;
    InvoiceDoc::from_snapshot(&snapshot, None)
}

/// The document of an issued invoice, from its snapshot.
pub(crate) fn issued_doc(conn: &Connection, id: Uuid) -> Result<InvoiceDoc> {
    let snapshot = crate::issue::load_snapshot(conn, id)?;
    let created_at: String = conn.query_row(
        "SELECT created_at FROM invoice_snapshots WHERE invoice_id = ?1",
        [id.to_string()],
        |r| r.get(0),
    )?;
    InvoiceDoc::from_snapshot(&snapshot, Some(&created_at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::invoice::{create_draft, save_draft};
    use crate::issue::{IssueOptions, issue_invoice, load_snapshot};
    use crate::masterdata::{create_business, create_customer};
    use crate::money::Cents;
    use crate::storno::cancel_invoice;
    use crate::testutil::{clock, d, sample_business, sample_customer, sample_draft};
    use rust_decimal::dec;

    fn draft(db: &mut Db) -> (Uuid, Uuid, Uuid) {
        let (_, brand) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let customer = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        let id = db
            .write(|tx| create_draft(tx, &clock(), sample_draft(brand, Some(customer))))
            .unwrap();
        (id, brand, customer)
    }

    fn issue(db: &mut Db, id: Uuid, payment_received: bool) {
        db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions { payment_received }))
            .unwrap();
    }

    #[test]
    fn doc_from_issued_snapshot() {
        let mut db = Db::open_in_memory().unwrap();
        let (id, _, _) = draft(&mut db);
        issue(&mut db, id, false);
        let doc = issued_doc(db.read(), id).unwrap();
        assert_eq!(doc.number.as_deref(), Some("RE-2026-0001"));
        assert_eq!(doc.totals.gross, Cents(2975));
        assert_eq!(doc.customer.as_ref().unwrap().name, "Kunde A");
        assert_eq!(doc.lines.len(), 1);
        assert!(doc.issued_at.is_some());
        assert!(!doc.payment_received);
    }

    #[test]
    fn draft_doc_has_no_number_and_current_totals() {
        let mut db = Db::open_in_memory().unwrap();
        let (id, brand, customer) = draft(&mut db);
        let doc = draft_doc(db.read(), id).unwrap();
        assert_eq!(doc.number, None);
        assert_eq!(doc.totals.gross, Cents(2975));
        assert_eq!(doc.issued_at, None);
        let mut input = sample_draft(brand, Some(customer));
        input.lines[0].quantity = dec!(4);
        db.write(|tx| save_draft(tx, &clock(), id, input)).unwrap();
        assert_eq!(draft_doc(db.read(), id).unwrap().totals.gross, Cents(5950));
        issue(&mut db, id, false);
        assert!(matches!(draft_doc(db.read(), id), Err(Error::NotDraft)));
    }

    #[test]
    fn snapshot_records_payment_received() {
        let mut db = Db::open_in_memory().unwrap();
        let (id, _, _) = draft(&mut db);
        issue(&mut db, id, true);
        assert!(issued_doc(db.read(), id).unwrap().payment_received);
    }

    #[test]
    fn old_snapshot_without_flag_reads_false() {
        let mut db = Db::open_in_memory().unwrap();
        let (id, _, _) = draft(&mut db);
        issue(&mut db, id, true);
        let mut snapshot = load_snapshot(db.read(), id).unwrap();
        snapshot.as_object_mut().unwrap().remove("payment_received");
        let doc = InvoiceDoc::from_snapshot(&snapshot, None).unwrap();
        assert!(!doc.payment_received);
    }

    #[test]
    fn storno_doc_names_original() {
        let mut db = Db::open_in_memory().unwrap();
        let (id, _, _) = draft(&mut db);
        issue(&mut db, id, false);
        let storno = db.write(|tx| cancel_invoice(tx, &clock(), id)).unwrap();
        let doc = issued_doc(db.read(), storno).unwrap();
        assert_eq!(doc.kind, InvoiceKind::Storno);
        assert_eq!(
            doc.cancels,
            Some(("RE-2026-0001".to_string(), d(2026, 11, 18)))
        );
        assert_eq!(doc.totals.gross, Cents(-2975));
    }
}
