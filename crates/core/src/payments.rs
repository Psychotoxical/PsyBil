//! Payments and the payment status of an invoice. The status is always
//! derived from the payments and the due date, never stored.

use chrono::NaiveDate;
use rusqlite::{Connection, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::audit;
use crate::dates::Clock;
use crate::db::{UnknownValue, not_found, opt_uuid_col, uuid_col};
use crate::invoice::{InvoiceKind, InvoiceState, load_invoice};
use crate::issue::insert_payment;
use crate::money::Cents;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    Transfer,
    Cash,
    PayPal,
    Other,
    /// Taken over from VibeBill.
    Legacy,
}

impl PaymentMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            PaymentMethod::Transfer => "transfer",
            PaymentMethod::Cash => "cash",
            PaymentMethod::PayPal => "paypal",
            PaymentMethod::Other => "other",
            PaymentMethod::Legacy => "legacy",
        }
    }

    pub fn parse(text: &str) -> Option<PaymentMethod> {
        match text {
            "transfer" => Some(PaymentMethod::Transfer),
            "cash" => Some(PaymentMethod::Cash),
            "paypal" => Some(PaymentMethod::PayPal),
            "other" => Some(PaymentMethod::Other),
            "legacy" => Some(PaymentMethod::Legacy),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payment {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub amount: Cents,
    pub date: NaiveDate,
    pub method: PaymentMethod,
    pub note: String,
    /// Set on a copy made when a correction took over the payment.
    pub transfer_of_id: Option<Uuid>,
    /// Set on the original when it was moved to a replacement invoice.
    pub transferred_to_id: Option<Uuid>,
}

const PAYMENT_COLS: &str =
    "id, invoice_id, amount_cents, date, method, note, transfer_of_id, transferred_to_id";

fn payment_from_row(r: &Row) -> rusqlite::Result<Payment> {
    let method: String = r.get(4)?;
    Ok(Payment {
        id: uuid_col(r, 0)?,
        invoice_id: uuid_col(r, 1)?,
        amount: Cents(r.get(2)?),
        date: r.get(3)?,
        method: PaymentMethod::parse(&method).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(UnknownValue(method.clone())),
            )
        })?,
        note: r.get(5)?,
        transfer_of_id: opt_uuid_col(r, 6)?,
        transferred_to_id: opt_uuid_col(r, 7)?,
    })
}

/// Records a payment on an issued invoice. Negative amounts are refunds.
pub fn add_payment(
    tx: &Transaction,
    clock: &dyn Clock,
    invoice_id: Uuid,
    amount: Cents,
    date: NaiveDate,
    method: PaymentMethod,
    note: &str,
) -> Result<Uuid> {
    if load_invoice(tx, invoice_id)?.state != InvoiceState::Issued {
        return Err(Error::NotIssued);
    }
    let id = insert_payment(
        tx,
        clock,
        invoice_id,
        amount,
        date,
        method.as_str(),
        note,
        None,
    )?;
    audit::log(
        tx,
        clock,
        "payment",
        id,
        "create",
        serde_json::json!({ "invoice_id": invoice_id, "amount": amount, "date": date }),
    )?;
    Ok(id)
}

/// Deletes a payment. A payment that was moved to a replacement invoice is
/// deleted there; deleting the moved copy also deletes its original, so the
/// money disappears from the receipts as well.
pub fn delete_payment(tx: &Transaction, clock: &dyn Clock, id: Uuid) -> Result<()> {
    let payment = get_payment(tx, id)?;
    if payment.transferred_to_id.is_some() {
        return Err(Error::PaymentTransferred);
    }
    tx.execute("DELETE FROM payments WHERE id = ?1", [id.to_string()])?;
    crate::tombstone::mark_deleted(tx, clock, "payments", id)?;
    audit::log(
        tx,
        clock,
        "payment",
        id,
        "delete",
        serde_json::to_value(&payment)?,
    )?;
    if let Some(original_id) = payment.transfer_of_id {
        let original = get_payment(tx, original_id)?;
        tx.execute(
            "DELETE FROM payments WHERE id = ?1",
            [original_id.to_string()],
        )?;
        crate::tombstone::mark_deleted(tx, clock, "payments", original_id)?;
        audit::log(
            tx,
            clock,
            "payment",
            original_id,
            "delete",
            serde_json::to_value(&original)?,
        )?;
    }
    Ok(())
}

fn get_payment(conn: &Connection, id: Uuid) -> Result<Payment> {
    conn.query_row(
        &format!("SELECT {PAYMENT_COLS} FROM payments WHERE id = ?1"),
        [id.to_string()],
        payment_from_row,
    )
    .map_err(not_found)
}

pub fn list_payments(conn: &Connection, invoice_id: Uuid) -> Result<Vec<Payment>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PAYMENT_COLS} FROM payments WHERE invoice_id = ?1 ORDER BY date, created_at"
    ))?;
    let rows = stmt.query_map([invoice_id.to_string()], payment_from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusKind {
    Draft,
    Open,
    PartlyPaid,
    Paid,
    Overpaid,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceStatus {
    pub kind: StatusKind,
    /// Still to pay; negative when overpaid.
    pub open: Cents,
    pub overdue: bool,
}

/// The payment status of an invoice on `today`.
pub fn invoice_status(conn: &Connection, id: Uuid, today: NaiveDate) -> Result<InvoiceStatus> {
    let invoice = load_invoice(conn, id)?;
    let settled = |kind| InvoiceStatus {
        kind,
        open: Cents::ZERO,
        overdue: false,
    };
    if invoice.state == InvoiceState::Draft {
        return Ok(settled(StatusKind::Draft));
    }
    if invoice.cancelled_by_id.is_some() || invoice.legacy_cancelled {
        return Ok(settled(StatusKind::Cancelled));
    }
    if invoice.kind == InvoiceKind::Storno {
        return Ok(settled(StatusKind::Paid));
    }
    let paid: i64 = conn.query_row(
        "SELECT COALESCE(SUM(amount_cents), 0) FROM payments
         WHERE invoice_id = ?1 AND transferred_to_id IS NULL",
        [id.to_string()],
        |r| r.get(0),
    )?;
    let paid = Cents(paid);
    let open = invoice.gross.unwrap_or(Cents::ZERO).checked_sub(paid)?;
    let kind = if open == Cents::ZERO {
        StatusKind::Paid
    } else if open < Cents::ZERO {
        StatusKind::Overpaid
    } else if paid == Cents::ZERO {
        StatusKind::Open
    } else {
        StatusKind::PartlyPaid
    };
    Ok(InvoiceStatus {
        kind,
        open,
        overdue: open > Cents::ZERO && today > invoice.due_date,
    })
}

/// Money received between `from` and `to` (inclusive). Payments moved to a
/// replacement invoice count once, on their original date.
pub fn receipts(
    conn: &Connection,
    business_id: Option<Uuid>,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Cents> {
    let total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(p.amount_cents), 0) FROM payments p
         JOIN invoices i ON i.id = p.invoice_id
         WHERE p.transfer_of_id IS NULL AND p.date BETWEEN ?1 AND ?2
           AND (?3 IS NULL OR i.business_id = ?3)",
        params![from, to, business_id.map(|b| b.to_string())],
        |r| r.get(0),
    )?;
    Ok(Cents(total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::invoice::create_draft;
    use crate::issue::{IssueOptions, insert_payment, issue_invoice};
    use crate::masterdata::{create_business, create_customer};
    use crate::testutil::{clock, d, sample_business, sample_customer, sample_draft};
    use rust_decimal::{Decimal, dec};

    /// An issued invoice of 1 × `net` at 19 %, dated 2026-11-18, due 2026-12-02.
    fn issued(net: Decimal) -> (Db, Uuid, Uuid) {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let customer_id = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        let mut input = sample_draft(brand_id, Some(customer_id));
        input.lines[0].quantity = dec!(1);
        input.lines[0].unit_price = net;
        let id = db.write(|tx| create_draft(tx, &clock(), input)).unwrap();
        db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()))
            .unwrap();
        (db, business_id, id)
    }

    fn pay(db: &mut Db, id: Uuid, cents: i64) -> Uuid {
        db.write(|tx| {
            add_payment(
                tx,
                &clock(),
                id,
                Cents(cents),
                d(2026, 11, 20),
                PaymentMethod::Transfer,
                "",
            )
        })
        .unwrap()
    }

    #[test]
    fn exact_cent_sum_is_paid() {
        let (mut db, _, id) = issued(dec!(30));
        pay(&mut db, id, 101);
        pay(&mut db, id, 3469);
        let status = invoice_status(db.read(), id, d(2026, 11, 20)).unwrap();
        assert_eq!(
            status,
            InvoiceStatus {
                kind: StatusKind::Paid,
                open: Cents(0),
                overdue: false
            }
        );
    }

    #[test]
    fn open_before_due_date() {
        let (db, _, id) = issued(dec!(25));
        let status = invoice_status(db.read(), id, d(2026, 12, 2)).unwrap();
        assert_eq!(
            status,
            InvoiceStatus {
                kind: StatusKind::Open,
                open: Cents(2975),
                overdue: false
            }
        );
    }

    #[test]
    fn partial_then_overdue() {
        let (mut db, _, id) = issued(dec!(25));
        pay(&mut db, id, 1000);
        let status = invoice_status(db.read(), id, d(2026, 12, 10)).unwrap();
        assert_eq!(
            status,
            InvoiceStatus {
                kind: StatusKind::PartlyPaid,
                open: Cents(1975),
                overdue: true
            }
        );
    }

    #[test]
    fn overpaid_reports_amount() {
        let (mut db, _, id) = issued(dec!(25));
        pay(&mut db, id, 3000);
        let status = invoice_status(db.read(), id, d(2026, 11, 20)).unwrap();
        assert_eq!(status.kind, StatusKind::Overpaid);
        assert_eq!(status.open, Cents(-25));
    }

    #[test]
    fn refund_is_negative_payment() {
        let (mut db, _, id) = issued(dec!(25));
        pay(&mut db, id, 2975);
        pay(&mut db, id, -2975);
        let status = invoice_status(db.read(), id, d(2026, 11, 20)).unwrap();
        assert_eq!(status.kind, StatusKind::Open);
        assert_eq!(status.open, Cents(2975));
        assert_eq!(list_payments(db.read(), id).unwrap().len(), 2);
    }

    #[test]
    fn payment_on_draft_refused() {
        let mut db = Db::open_in_memory().unwrap();
        let (_, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let id = db
            .write(|tx| create_draft(tx, &clock(), sample_draft(brand_id, None)))
            .unwrap();
        let result = db.write(|tx| {
            add_payment(
                tx,
                &clock(),
                id,
                Cents(100),
                d(2026, 11, 20),
                PaymentMethod::Cash,
                "",
            )
        });
        assert!(matches!(result, Err(Error::NotIssued)));
        assert_eq!(
            invoice_status(db.read(), id, d(2026, 11, 20)).unwrap().kind,
            StatusKind::Draft
        );
    }

    #[test]
    fn delete_payment_is_audited() {
        let (mut db, _, id) = issued(dec!(25));
        let payment = pay(&mut db, id, 500);
        db.write(|tx| delete_payment(tx, &clock(), payment))
            .unwrap();
        assert!(list_payments(db.read(), id).unwrap().is_empty());
        let count: i64 = db
            .read()
            .query_row(
                "SELECT COUNT(*) FROM audit_log WHERE entity = 'payment' AND action = 'delete'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn receipts_exclude_transfers() {
        let (mut db, business_id, id) = issued(dec!(25));
        let original = pay(&mut db, id, 1000);
        db.write(|tx| {
            insert_payment(
                tx,
                &clock(),
                id,
                Cents(500),
                d(2026, 11, 21),
                "transfer",
                "",
                Some(original),
            )
        })
        .unwrap();
        let total = receipts(
            db.read(),
            Some(business_id),
            d(2026, 11, 1),
            d(2026, 11, 30),
        )
        .unwrap();
        assert_eq!(total, Cents(1000));
        let outside = receipts(db.read(), None, d(2026, 12, 1), d(2026, 12, 31)).unwrap();
        assert_eq!(outside, Cents(0));
    }
}
