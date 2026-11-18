//! Issuing: a draft gets its number and is frozen for good.

use chrono::{Datelike, NaiveDate};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::audit;
use crate::calc::Totals;
use crate::dates::{Clock, timestamp};
use crate::invoice::{Invoice, InvoiceKind, InvoiceState, draft_totals, load_invoice};
use crate::masterdata::{ProductKind, get_brand, get_business, get_customer, get_product};
use crate::money::Cents;
use crate::numbering::allocate_number;
use crate::tax::{TaxCategory, TaxChoice};
use crate::validate::{Severity, check_issue};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IssueOptions {
    /// The full amount has already been paid; records a payment dated on
    /// the invoice date.
    pub payment_received: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issued {
    pub number: String,
    pub totals: Totals,
    /// Products whose stock fell below zero. A warning, not an error.
    pub negative_stock: Vec<Uuid>,
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Inserts a payment row. Used by issuing and by the payments module.
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_payment(
    tx: &Transaction,
    clock: &dyn Clock,
    invoice_id: Uuid,
    amount: Cents,
    date: NaiveDate,
    method: &str,
    note: &str,
    transfer_of_id: Option<Uuid>,
) -> Result<Uuid> {
    let id = Uuid::new_v4();
    let now = timestamp(clock);
    tx.execute(
        "INSERT INTO payments (id, invoice_id, amount_cents, date, method, note, transfer_of_id,
            created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            id.to_string(),
            invoice_id.to_string(),
            amount.0,
            date,
            method,
            note,
            transfer_of_id.map(|t| t.to_string()),
            now,
        ],
    )?;
    Ok(id)
}

/// Writers take the transaction from `Db::write`, never the read connection:
///
/// ```compile_fail
/// use psybil_core::db::Db;
/// use psybil_core::dates::SystemClock;
/// use psybil_core::issue::{IssueOptions, issue_invoice};
/// let db = Db::open_in_memory().unwrap();
/// let id = uuid::Uuid::nil();
/// let _ = issue_invoice(db.read(), &SystemClock, id, IssueOptions::default());
/// ```
///
/// Issues a draft: checks the mandatory contents, assigns the number,
/// freezes a snapshot, books the stock and writes the audit log, all in the
/// caller's transaction.
pub fn issue_invoice(
    tx: &Transaction,
    clock: &dyn Clock,
    id: Uuid,
    opts: IssueOptions,
) -> Result<Issued> {
    let mut invoice = load_invoice(tx, id)?;
    if invoice.state != InvoiceState::Draft {
        return Err(Error::AlreadyIssued);
    }
    let business = get_business(tx, invoice.business_id)?;
    let totals = match (invoice.kind, invoice.cancels_id) {
        // A cancellation mirrors its original: no checks against today's
        // master data, and exactly the negated frozen totals.
        (InvoiceKind::Storno, Some(original)) => {
            let frozen = load_snapshot(tx, original)?;
            serde_json::from_value::<Totals>(frozen["totals"].clone())?.negated()
        }
        _ => {
            let issues = check_issue(tx, id)?;
            if issues.iter().any(|i| i.severity == Severity::Error) {
                return Err(Error::Validation(issues));
            }
            if business.small_business {
                normalise_small_business_lines(tx, &mut invoice)?;
            }
            draft_totals(tx, id)?
        }
    };

    let brand = get_brand(tx, invoice.brand_id)?;
    let prefix = match invoice.kind {
        InvoiceKind::Invoice => brand.invoice_prefix.as_str(),
        InvoiceKind::Storno => business.storno_prefix.as_str(),
    };
    let number = allocate_number(tx, business.id, prefix, invoice.invoice_date.year())?;
    let snapshot = build_snapshot(tx, &invoice, &number, &totals, false)?;

    let now = timestamp(clock);
    tx.execute(
        "UPDATE invoices SET state = 'issued', number = ?2, net_cents = ?3, tax_cents = ?4,
            gross_cents = ?5, updated_at = ?6
         WHERE id = ?1",
        params![
            id.to_string(),
            number,
            totals.net.0,
            totals.tax.0,
            totals.gross.0,
            now
        ],
    )?;
    tx.execute(
        "INSERT INTO invoice_snapshots (invoice_id, snapshot, created_at) VALUES (?1, ?2, ?3)",
        params![id.to_string(), snapshot.to_string(), now],
    )?;

    let mut negative_stock = Vec::new();
    for line in &invoice.lines {
        let Some(product_id) = line.product_id else {
            continue;
        };
        let product = get_product(tx, product_id)?;
        let (ProductKind::Goods, Some(stock)) = (product.kind, product.stock) else {
            continue;
        };
        // Only the booking is written; the effective stock is base + bookings.
        let delta: Decimal = -line.quantity;
        let new_stock = stock + delta;
        tx.execute(
            "INSERT INTO stock_moves (id, product_id, invoice_id, delta, at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![Uuid::new_v4().to_string(), product_id.to_string(), id.to_string(), delta.to_string(), now],
        )?;
        if new_stock < Decimal::ZERO && !negative_stock.contains(&product_id) {
            negative_stock.push(product_id);
        }
    }

    audit::log(
        tx,
        clock,
        "invoice",
        id,
        "issue",
        serde_json::json!({ "number": number, "gross": totals.gross }),
    )?;
    let transferred = match invoice.replaces_id {
        Some(original) => transfer_payments(tx, clock, original, id)?,
        None => Cents::ZERO,
    };
    // Only what the moved payments do not already cover is a new receipt.
    let still_open = totals.gross.checked_sub(transferred)?;
    if opts.payment_received && still_open > Cents::ZERO {
        insert_payment(
            tx,
            clock,
            id,
            still_open,
            invoice.invoice_date,
            "other",
            "",
            None,
        )?;
    }
    Ok(Issued {
        number,
        totals,
        negative_stock,
    })
}

/// The frozen picture of an invoice: master data, lines, totals and texts as
/// they are now. `imported` marks invoices taken over from VibeBill.
pub(crate) fn build_snapshot(
    tx: &Transaction,
    invoice: &Invoice,
    number: &str,
    totals: &Totals,
    imported: bool,
) -> Result<serde_json::Value> {
    let business = get_business(tx, invoice.business_id)?;
    let brand = get_brand(tx, invoice.brand_id)?;
    let customer = invoice
        .customer_id
        .map(|c| get_customer(tx, c))
        .transpose()?;
    let cancels = invoice
        .cancels_id
        .map(|original| {
            tx.query_row(
                "SELECT number, invoice_date FROM invoices WHERE id = ?1",
                [original.to_string()],
                |r| Ok(serde_json::json!({ "number": r.get::<_, String>(0)?, "invoice_date": r.get::<_, String>(1)? })),
            )
            .optional()
        })
        .transpose()?
        .flatten();
    Ok(serde_json::json!({
        "number": number,
        "kind": invoice.kind,
        "invoice_date": invoice.invoice_date,
        "service_date": invoice.service_date,
        "due_date": invoice.due_date,
        "currency": invoice.currency,
        "language": invoice.language,
        "price_mode": invoice.price_mode,
        "pdf_template": brand.pdf_template,
        "business": business,
        "brand": brand,
        "logo_sha256": brand.logo.as_deref().map(sha256_hex),
        "customer": customer,
        "lines": invoice.lines,
        "totals": totals,
        "texts": { "intro": invoice.intro_text, "outro": invoice.outro_text, "notes": invoice.notes },
        "cancels": cancels,
        "imported": imported,
    }))
}

/// Sets every line of a small-business invoice to the § 19 category, so the
/// frozen lines never show a VAT rate (§ 14c UStG).
fn normalise_small_business_lines(tx: &Transaction, invoice: &mut Invoice) -> Result<()> {
    let tax = TaxChoice::Fixed(TaxCategory::SmallBusiness);
    tx.execute(
        "UPDATE invoice_lines SET tax = ?2 WHERE invoice_id = ?1",
        params![invoice.id.to_string(), serde_json::to_string(&tax)?],
    )?;
    for line in &mut invoice.lines {
        line.tax = tax.clone();
    }
    Ok(())
}

/// Moves the payments of a corrected invoice to its replacement. The copies
/// keep date and amount and are not counted as new receipts. Returns the
/// moved total.
fn transfer_payments(tx: &Transaction, clock: &dyn Clock, from: Uuid, to: Uuid) -> Result<Cents> {
    let mut moved = Cents::ZERO;
    let mut stmt = tx.prepare(
        "SELECT id, amount_cents, date, method, note FROM payments
         WHERE invoice_id = ?1 AND transferred_to_id IS NULL ORDER BY date, created_at",
    )?;
    let payments = stmt
        .query_map([from.to_string()], |r| {
            Ok((
                crate::db::uuid_col(r, 0)?,
                Cents(r.get(1)?),
                r.get::<_, NaiveDate>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (payment_id, amount, date, method, note) in payments {
        moved = moved.checked_add(amount)?;
        insert_payment(
            tx,
            clock,
            to,
            amount,
            date,
            &method,
            &note,
            Some(payment_id),
        )?;
        tx.execute(
            "UPDATE payments SET transferred_to_id = ?2, updated_at = ?3 WHERE id = ?1",
            params![payment_id.to_string(), to.to_string(), timestamp(clock)],
        )?;
        audit::log(
            tx,
            clock,
            "payment",
            payment_id,
            "transfer",
            serde_json::json!({ "from": from, "to": to, "amount": amount }),
        )?;
    }
    Ok(moved)
}

/// The frozen snapshot of an issued invoice.
pub fn load_snapshot(conn: &Connection, id: Uuid) -> Result<serde_json::Value> {
    let text: String = conn
        .query_row(
            "SELECT snapshot FROM invoice_snapshots WHERE invoice_id = ?1",
            [id.to_string()],
            |r| r.get(0),
        )
        .map_err(crate::db::not_found)?;
    Ok(serde_json::from_str(&text)?)
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use crate::db::Db;
    use crate::invoice::create_draft;
    use crate::masterdata::{NewBusiness, create_business, create_customer};
    use crate::tax::{TaxCategory, TaxChoice};
    use crate::testutil::{clock, sample_business, sample_customer, sample_draft};

    #[test]
    fn small_business_issue_normalises_line_tax() {
        let mut db = Db::open_in_memory().unwrap();
        let business = NewBusiness {
            small_business: true,
            ..sample_business()
        };
        let (_, brand_id) = db
            .write(|tx| create_business(tx, &clock(), business))
            .unwrap();
        let customer_id = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        let id = db
            .write(|tx| create_draft(tx, &clock(), sample_draft(brand_id, Some(customer_id))))
            .unwrap();
        db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()))
            .unwrap();
        let invoice = load_invoice(db.read(), id).unwrap();
        assert_eq!(
            invoice.lines[0].tax,
            TaxChoice::Fixed(TaxCategory::SmallBusiness)
        );
        let snapshot = load_snapshot(db.read(), id).unwrap();
        assert_eq!(
            snapshot["lines"][0]["tax"]["category"]["type"],
            "small_business"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{LineKind, PriceMode};
    use crate::dates::FixedClock;
    use crate::db::Db;
    use crate::invoice::{LineDraft, create_draft, save_draft};
    use crate::masterdata::{
        NewBusiness, NewProduct, ProductKind, create_business, create_customer, create_product,
        get_customer, get_product, update_customer,
    };
    use crate::tax::{TaxCategory, TaxChoice};
    use crate::testutil::{clock, d, sample_business, sample_customer, sample_draft};
    use rust_decimal::dec;

    struct Setup {
        db: Db,
        business_id: Uuid,
        brand_id: Uuid,
        customer_id: Uuid,
    }

    fn setup(business: NewBusiness) -> Setup {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id) = db
            .write(|tx| create_business(tx, &clock(), business))
            .unwrap();
        let customer_id = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        Setup {
            db,
            business_id,
            brand_id,
            customer_id,
        }
    }

    fn draft(s: &mut Setup) -> Uuid {
        let input = sample_draft(s.brand_id, Some(s.customer_id));
        s.db.write(|tx| create_draft(tx, &clock(), input)).unwrap()
    }

    fn counter_next(db: &Db, business_id: Uuid, year: i32) -> Option<i64> {
        db.read()
            .query_row(
                "SELECT next FROM number_counters WHERE business_id = ?1 AND year = ?2",
                rusqlite::params![business_id.to_string(), year],
                |r| r.get(0),
            )
            .ok()
    }

    #[test]
    fn issue_assigns_number_and_freezes() {
        let mut s = setup(sample_business());
        let id = draft(&mut s);
        let issued =
            s.db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()))
                .unwrap();
        assert_eq!(issued.number, "RE-2026-0001");
        assert_eq!(issued.totals.gross, Cents(2975));
        let invoice = load_invoice(s.db.read(), id).unwrap();
        assert_eq!(invoice.state, InvoiceState::Issued);
        assert_eq!(invoice.number.as_deref(), Some("RE-2026-0001"));
        assert_eq!(invoice.gross, Some(Cents(2975)));
        let snapshot = load_snapshot(s.db.read(), id).unwrap();
        assert_eq!(snapshot["number"], "RE-2026-0001");
        let input = sample_draft(s.brand_id, Some(s.customer_id));
        assert!(matches!(
            s.db.write(|tx| save_draft(tx, &clock(), id, input)),
            Err(Error::NotDraft)
        ));
        assert!(
            s.db.read()
                .execute(
                    "UPDATE invoice_lines SET description = 'x' WHERE invoice_id = ?1",
                    [id.to_string()]
                )
                .is_err()
        );
    }

    #[test]
    fn issue_twice_is_refused() {
        let mut s = setup(sample_business());
        let id = draft(&mut s);
        s.db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()))
            .unwrap();
        let again =
            s.db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()));
        assert!(matches!(again, Err(Error::AlreadyIssued)));
        assert_eq!(counter_next(&s.db, s.business_id, 2026), Some(2));
    }

    #[test]
    fn issue_with_errors_changes_nothing() {
        let mut s = setup(NewBusiness {
            tax_number: String::new(),
            ..sample_business()
        });
        let id = draft(&mut s);
        let result =
            s.db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()));
        assert!(matches!(result, Err(Error::Validation(_))));
        assert_eq!(counter_next(&s.db, s.business_id, 2026), None);
        assert_eq!(
            load_invoice(s.db.read(), id).unwrap().state,
            InvoiceState::Draft
        );
    }

    #[test]
    fn snapshot_survives_master_data_change() {
        let mut s = setup(sample_business());
        let id = draft(&mut s);
        s.db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()))
            .unwrap();
        let mut customer = get_customer(s.db.read(), s.customer_id).unwrap();
        customer.street = "Neue Straße 9".into();
        s.db.write(|tx| update_customer(tx, &clock(), &customer))
            .unwrap();
        let snapshot = load_snapshot(s.db.read(), id).unwrap();
        assert_eq!(snapshot["customer"]["street"], "Hauptstraße 2");
    }

    #[test]
    fn stock_is_reduced_for_goods_only() {
        let mut s = setup(sample_business());
        let product = |kind: ProductKind, stock| NewProduct {
            brand_id: s.brand_id,
            name: "Artikel".into(),
            description: String::new(),
            kind,
            unit: "Stk".into(),
            price: dec!(10),
            price_mode: PriceMode::Net,
            tax: TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(19) }),
            stock,
            active: true,
        };
        let goods = product(ProductKind::Goods, Some(dec!(1)));
        let service = product(ProductKind::Service, None);
        let goods_id =
            s.db.write(|tx| create_product(tx, &clock(), goods))
                .unwrap();
        let service_id =
            s.db.write(|tx| create_product(tx, &clock(), service))
                .unwrap();
        let mut input = sample_draft(s.brand_id, Some(s.customer_id));
        let line = |product_id, quantity| LineDraft {
            kind: LineKind::Item,
            product_id: Some(product_id),
            description: "Artikel".into(),
            quantity,
            unit: "Stk".into(),
            unit_price: dec!(10),
            tax: TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(19) }),
        };
        input.lines = vec![line(goods_id, dec!(2)), line(service_id, dec!(1))];
        let id = s.db.write(|tx| create_draft(tx, &clock(), input)).unwrap();
        let issued =
            s.db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()))
                .unwrap();
        assert_eq!(
            get_product(s.db.read(), goods_id).unwrap().stock,
            Some(dec!(-1))
        );
        assert_eq!(get_product(s.db.read(), service_id).unwrap().stock, None);
        assert_eq!(issued.negative_stock, vec![goods_id]);
    }

    #[test]
    fn payment_received_creates_payment() {
        let mut s = setup(sample_business());
        let id = draft(&mut s);
        s.db.write(|tx| {
            issue_invoice(
                tx,
                &clock(),
                id,
                IssueOptions {
                    payment_received: true,
                },
            )
        })
        .unwrap();
        let (amount, date): (i64, String) =
            s.db.read()
                .query_row(
                    "SELECT amount_cents, date FROM payments WHERE invoice_id = ?1",
                    [id.to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
        assert_eq!((amount, date.as_str()), (2975, "2026-11-18"));
    }

    #[test]
    fn december_invoice_issued_in_january() {
        let mut s = setup(sample_business());
        let mut input = sample_draft(s.brand_id, Some(s.customer_id));
        input.invoice_date = d(2025, 12, 31);
        let id = s.db.write(|tx| create_draft(tx, &clock(), input)).unwrap();
        let january = FixedClock {
            today: d(2026, 1, 5),
        };
        let issued =
            s.db.write(|tx| issue_invoice(tx, &january, id, IssueOptions::default()))
                .unwrap();
        assert_eq!(issued.number, "RE-2025-0001");
    }
}
