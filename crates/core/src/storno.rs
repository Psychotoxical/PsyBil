//! Cancellation (Storno) and correction of issued invoices. An issued
//! invoice is never changed; it is cancelled by a document of its own that
//! refers to it (§ 31 Abs. 5 UStDV).

use rusqlite::{Transaction, params};
use uuid::Uuid;

use crate::audit;
use crate::dates::{Clock, ServiceDate, timestamp};
use crate::invoice::{
    DraftInput, Invoice, InvoiceKind, InvoiceState, LineDraft, create_draft, duplicate_as_draft,
    load_invoice,
};
use crate::issue::{IssueOptions, issue_invoice};
use crate::{Error, Result};

/// Cancels an issued invoice. Creates and issues the cancellation document
/// and returns its id.
pub fn cancel_invoice(tx: &Transaction, clock: &dyn Clock, id: Uuid) -> Result<Uuid> {
    let original = load_invoice(tx, id)?;
    if original.state != InvoiceState::Issued
        || original.kind != InvoiceKind::Invoice
        || original.cancelled_by_id.is_some()
        || original.legacy_cancelled
    {
        return Err(Error::NotCancellable);
    }
    let lines = original
        .lines
        .iter()
        .map(|line| LineDraft {
            quantity: -line.quantity,
            ..line.clone()
        })
        .collect();
    let storno = create_draft(
        tx,
        clock,
        DraftInput {
            brand_id: original.brand_id,
            customer_id: original.customer_id,
            invoice_date: clock.today(),
            service_date: resolved_service_date(&original),
            payment_days: 0,
            price_mode: original.price_mode,
            language: original.language.clone(),
            intro_text: String::new(),
            outro_text: String::new(),
            notes: String::new(),
            lines,
        },
    )?;
    // The cancellation belongs to the original's business and currency,
    // whatever the master data says today.
    tx.execute(
        "UPDATE invoices SET kind = 'storno', cancels_id = ?2, business_id = ?3, currency = ?4
         WHERE id = ?1",
        params![
            storno.to_string(),
            id.to_string(),
            original.business_id.to_string(),
            original.currency
        ],
    )?;
    issue_invoice(tx, clock, storno, IssueOptions::default())?;
    tx.execute(
        "UPDATE invoices SET cancelled_by_id = ?2, updated_at = ?3 WHERE id = ?1",
        params![id.to_string(), storno.to_string(), timestamp(clock)],
    )?;
    audit::log(
        tx,
        clock,
        "invoice",
        id,
        "cancel",
        serde_json::json!({ "storno": storno }),
    )?;
    Ok(storno)
}

/// Cancels an issued invoice and starts a replacement draft with the same
/// contents. When the replacement is issued, the payments of the original
/// move over to it. Returns `(storno id, draft id)`.
pub fn correct_invoice(tx: &Transaction, clock: &dyn Clock, id: Uuid) -> Result<(Uuid, Uuid)> {
    let original = load_invoice(tx, id)?;
    let storno = cancel_invoice(tx, clock, id)?;
    let draft = duplicate_as_draft(tx, clock, id)?;
    // The replacement describes the same service, so it keeps its date.
    tx.execute(
        "UPDATE invoices SET replaces_id = ?2, service_date = ?3 WHERE id = ?1",
        params![
            draft.to_string(),
            id.to_string(),
            serde_json::to_string(&resolved_service_date(&original))?
        ],
    )?;
    Ok((storno, draft))
}

/// The original's service date, with "same as invoice date" made explicit,
/// because the new document has a different invoice date.
fn resolved_service_date(original: &Invoice) -> ServiceDate {
    match original.service_date {
        ServiceDate::SameAsInvoice => ServiceDate::Day {
            date: original.invoice_date,
        },
        other => other,
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use crate::dates::ServiceDate;
    use crate::db::Db;
    use crate::invoice::{create_draft, load_invoice};
    use crate::issue::{IssueOptions, issue_invoice};
    use crate::masterdata::{create_business, create_customer, get_business, update_business};
    use crate::money::Cents;
    use crate::payments::{
        PaymentMethod, StatusKind, add_payment, delete_payment, invoice_status, list_payments,
        receipts,
    };
    use crate::testutil::{clock, d, sample_business, sample_customer, sample_draft};
    use rust_decimal::dec;

    struct Setup {
        db: Db,
        business_id: Uuid,
        brand_id: Uuid,
        customer_id: Uuid,
    }

    fn setup() -> Setup {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
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

    /// Issues 1 × 100 € at 19 % (gross 119,00).
    fn issue_hundred(
        s: &mut Setup,
        customer: Option<Uuid>,
        edit: impl FnOnce(&mut DraftInput),
    ) -> Uuid {
        let mut input = sample_draft(s.brand_id, customer);
        input.lines[0].quantity = dec!(1);
        input.lines[0].unit_price = dec!(100);
        edit(&mut input);
        let id = s.db.write(|tx| create_draft(tx, &clock(), input)).unwrap();
        s.db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()))
            .unwrap();
        id
    }

    fn switch_to_small_business(s: &mut Setup) {
        let mut business = get_business(s.db.read(), s.business_id).unwrap();
        business.small_business = true;
        s.db.write(|tx| update_business(tx, &clock(), &business))
            .unwrap();
    }

    #[test]
    fn storno_mirrors_original_after_small_business_switch() {
        let mut s = setup();
        let customer = Some(s.customer_id);
        let id = issue_hundred(&mut s, customer, |_| {});
        switch_to_small_business(&mut s);
        let storno = s.db.write(|tx| cancel_invoice(tx, &clock(), id)).unwrap();
        let doc = load_invoice(s.db.read(), storno).unwrap();
        assert_eq!(doc.gross, Some(Cents(-11900)));
        assert_eq!(doc.tax, Some(Cents(-1900)));
    }

    #[test]
    fn storno_ignores_current_master_data_checks() {
        let mut s = setup();
        let id = issue_hundred(&mut s, None, |_| {});
        switch_to_small_business(&mut s);
        assert!(s.db.write(|tx| cancel_invoice(tx, &clock(), id)).is_ok());
    }

    #[test]
    fn correction_keeps_original_service_date() {
        let mut s = setup();
        let customer = Some(s.customer_id);
        let id = issue_hundred(&mut s, customer, |input| {
            input.invoice_date = d(2026, 3, 10);
        });
        let (storno, replacement) = s.db.write(|tx| correct_invoice(tx, &clock(), id)).unwrap();
        let expected = ServiceDate::Day {
            date: d(2026, 3, 10),
        };
        assert_eq!(
            load_invoice(s.db.read(), replacement).unwrap().service_date,
            expected
        );
        assert_eq!(
            load_invoice(s.db.read(), storno).unwrap().service_date,
            expected
        );
    }

    fn paid_and_corrected(s: &mut Setup, opts: IssueOptions) -> (Uuid, Uuid) {
        let customer = Some(s.customer_id);
        let id = issue_hundred(s, customer, |_| {});
        s.db.write(|tx| {
            add_payment(
                tx,
                &clock(),
                id,
                Cents(11900),
                d(2026, 11, 18),
                PaymentMethod::Transfer,
                "",
            )
        })
        .unwrap();
        let (_, replacement) = s.db.write(|tx| correct_invoice(tx, &clock(), id)).unwrap();
        s.db.write(|tx| issue_invoice(tx, &clock(), replacement, opts))
            .unwrap();
        (id, replacement)
    }

    #[test]
    fn correction_with_payment_received_counts_once() {
        let mut s = setup();
        let (_, replacement) = paid_and_corrected(
            &mut s,
            IssueOptions {
                payment_received: true,
            },
        );
        let status = invoice_status(s.db.read(), replacement, d(2026, 11, 18)).unwrap();
        assert_eq!(status.kind, StatusKind::Paid);
        let total = receipts(
            s.db.read(),
            Some(s.business_id),
            d(2026, 11, 1),
            d(2026, 11, 30),
        )
        .unwrap();
        assert_eq!(total, Cents(11900));
    }

    #[test]
    fn deleting_transferred_copy_removes_original() {
        let mut s = setup();
        let (original, replacement) = paid_and_corrected(&mut s, IssueOptions::default());
        let copy = list_payments(s.db.read(), replacement).unwrap().remove(0);
        s.db.write(|tx| delete_payment(tx, &clock(), copy.id))
            .unwrap();
        assert!(list_payments(s.db.read(), original).unwrap().is_empty());
        let total = receipts(s.db.read(), None, d(2026, 11, 1), d(2026, 11, 30)).unwrap();
        assert_eq!(total, Cents(0));
    }

    #[test]
    fn deleting_transferred_original_is_refused() {
        let mut s = setup();
        let (original, _) = paid_and_corrected(&mut s, IssueOptions::default());
        let moved = list_payments(s.db.read(), original).unwrap().remove(0);
        let result = s.db.write(|tx| delete_payment(tx, &clock(), moved.id));
        assert!(matches!(result, Err(Error::PaymentTransferred)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{LineKind, PriceMode};
    use crate::db::Db;
    use crate::invoice::{LineDraft, create_draft, load_invoice};
    use crate::issue::{IssueOptions, issue_invoice, load_snapshot};
    use crate::masterdata::{
        NewProduct, ProductKind, create_business, create_customer, create_product, get_product,
    };
    use crate::money::Cents;
    use crate::payments::{PaymentMethod, StatusKind, add_payment, invoice_status, receipts};
    use crate::tax::{TaxCategory, TaxChoice};
    use crate::testutil::{clock, d, sample_business, sample_customer, sample_draft};
    use rust_decimal::dec;

    struct Setup {
        db: Db,
        business_id: Uuid,
        brand_id: Uuid,
        customer_id: Uuid,
    }

    fn setup() -> Setup {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
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

    fn issued_invoice(s: &mut Setup, lines: Option<Vec<LineDraft>>) -> Uuid {
        let mut input = sample_draft(s.brand_id, Some(s.customer_id));
        if let Some(lines) = lines {
            input.lines = lines;
        }
        let id = s.db.write(|tx| create_draft(tx, &clock(), input)).unwrap();
        s.db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()))
            .unwrap();
        id
    }

    #[test]
    fn storno_gets_own_number() {
        let mut s = setup();
        let id = issued_invoice(&mut s, None);
        let storno = s.db.write(|tx| cancel_invoice(tx, &clock(), id)).unwrap();
        let doc = load_invoice(s.db.read(), storno).unwrap();
        assert_eq!(doc.number.as_deref(), Some("ST-2026-0002"));
        assert_eq!(doc.cancels_id, Some(id));
        assert_eq!(
            load_invoice(s.db.read(), id).unwrap().cancelled_by_id,
            Some(storno)
        );
        assert_eq!(
            invoice_status(s.db.read(), id, d(2026, 11, 18))
                .unwrap()
                .kind,
            StatusKind::Cancelled
        );
        let snapshot = load_snapshot(s.db.read(), storno).unwrap();
        assert_eq!(snapshot["cancels"]["number"], "RE-2026-0001");
    }

    #[test]
    fn storno_amounts_are_negative() {
        let mut s = setup();
        let id = issued_invoice(&mut s, None);
        let storno = s.db.write(|tx| cancel_invoice(tx, &clock(), id)).unwrap();
        let original = load_invoice(s.db.read(), id).unwrap();
        let doc = load_invoice(s.db.read(), storno).unwrap();
        assert_eq!(doc.gross, original.gross.map(|g| -g));
        assert_eq!(doc.gross, Some(Cents(-2975)));
    }

    #[test]
    fn storno_restores_stock() {
        let mut s = setup();
        let product = NewProduct {
            brand_id: s.brand_id,
            name: "Tasse".into(),
            description: String::new(),
            kind: ProductKind::Goods,
            unit: "Stk".into(),
            price: dec!(8),
            price_mode: PriceMode::Net,
            tax: TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(19) }),
            stock: Some(dec!(5)),
            active: true,
        };
        let product_id =
            s.db.write(|tx| create_product(tx, &clock(), product))
                .unwrap();
        let line = LineDraft {
            kind: LineKind::Item,
            product_id: Some(product_id),
            description: "Tasse".into(),
            quantity: dec!(2),
            unit: "Stk".into(),
            unit_price: dec!(8),
            tax: TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(19) }),
        };
        let id = issued_invoice(&mut s, Some(vec![line]));
        assert_eq!(
            get_product(s.db.read(), product_id).unwrap().stock,
            Some(dec!(3))
        );
        s.db.write(|tx| cancel_invoice(tx, &clock(), id)).unwrap();
        assert_eq!(
            get_product(s.db.read(), product_id).unwrap().stock,
            Some(dec!(5))
        );
    }

    #[test]
    fn cannot_cancel_twice() {
        let mut s = setup();
        let id = issued_invoice(&mut s, None);
        let storno = s.db.write(|tx| cancel_invoice(tx, &clock(), id)).unwrap();
        assert!(matches!(
            s.db.write(|tx| cancel_invoice(tx, &clock(), id)),
            Err(Error::NotCancellable)
        ));
        assert!(matches!(
            s.db.write(|tx| cancel_invoice(tx, &clock(), storno)),
            Err(Error::NotCancellable)
        ));
        let draft = s
            .db
            .write(|tx| create_draft(tx, &clock(), sample_draft(s.brand_id, Some(s.customer_id))))
            .unwrap();
        assert!(matches!(
            s.db.write(|tx| cancel_invoice(tx, &clock(), draft)),
            Err(Error::NotCancellable)
        ));
    }

    #[test]
    fn correction_moves_payments() {
        let mut s = setup();
        let mut line = sample_draft(s.brand_id, None).lines.remove(0);
        line.quantity = dec!(1);
        line.unit_price = dec!(100);
        let id = issued_invoice(&mut s, Some(vec![line]));
        s.db.write(|tx| {
            add_payment(
                tx,
                &clock(),
                id,
                Cents(11900),
                d(2026, 11, 18),
                PaymentMethod::Transfer,
                "",
            )
        })
        .unwrap();
        let (_, replacement) = s.db.write(|tx| correct_invoice(tx, &clock(), id)).unwrap();
        assert_eq!(
            load_invoice(s.db.read(), replacement).unwrap().replaces_id,
            Some(id)
        );
        s.db.write(|tx| issue_invoice(tx, &clock(), replacement, IssueOptions::default()))
            .unwrap();
        let status = invoice_status(s.db.read(), replacement, d(2026, 11, 18)).unwrap();
        assert_eq!(status.kind, StatusKind::Paid);
        let total = receipts(
            s.db.read(),
            Some(s.business_id),
            d(2026, 11, 1),
            d(2026, 11, 30),
        )
        .unwrap();
        assert_eq!(total, Cents(11900));
    }
}
