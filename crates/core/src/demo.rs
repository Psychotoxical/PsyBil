//! Neutral sample data for the demo mode (screenshots, trying things out).
//! No real names, places or numbers.

use rusqlite::Transaction;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::Result;
use crate::calc::{LineKind, PriceMode};
use crate::dates::{Clock, ServiceDate};
use crate::invoice::{DraftInput, LineDraft, create_draft};
use crate::issue::{IssueOptions, issue_invoice};
use crate::masterdata::{
    CustomerKind, NewBusiness, NewCustomer, NewProduct, ProductKind, create_business,
    create_customer, create_product, get_brand, update_brand,
};
use crate::money::Cents;
use crate::payments::{PaymentMethod, add_payment};
use crate::storno::cancel_invoice;
use crate::tax::{TaxCategory, TaxChoice};

static LOGO: &[u8] = include_bytes!("pdf/testdata/logo-alpha.png");

fn customer(tx: &Transaction, clock: &dyn Clock, name: &str, kind: CustomerKind) -> Result<Uuid> {
    create_customer(
        tx,
        clock,
        NewCustomer {
            name: name.into(),
            street: "Hauptstraße 2".into(),
            zip: "54321".into(),
            city: "Beispielstadt".into(),
            kind,
            ..NewCustomer::default()
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn product(
    tx: &Transaction,
    clock: &dyn Clock,
    brand: Uuid,
    name: &str,
    kind: ProductKind,
    unit: &str,
    price: Decimal,
    stock: Option<Decimal>,
    tax: TaxChoice,
) -> Result<Uuid> {
    create_product(
        tx,
        clock,
        NewProduct {
            brand_id: brand,
            name: name.into(),
            description: String::new(),
            kind,
            unit: unit.into(),
            price,
            price_mode: PriceMode::Net,
            tax,
            stock,
            active: true,
        },
    )
}

fn draft(
    tx: &Transaction,
    clock: &dyn Clock,
    brand: Uuid,
    customer: Uuid,
    days_ago: i64,
    line: (&str, Option<Uuid>, i64, &str, Decimal),
    tax: TaxChoice,
) -> Result<Uuid> {
    let (description, product_id, quantity, unit, price) = line;
    create_draft(
        tx,
        clock,
        DraftInput {
            brand_id: brand,
            customer_id: Some(customer),
            invoice_date: clock.today() - chrono::Duration::days(days_ago),
            service_date: ServiceDate::SameAsInvoice,
            payment_days: 14,
            price_mode: PriceMode::Net,
            language: "de".into(),
            intro_text: String::new(),
            outro_text: String::new(),
            notes: String::new(),
            lines: vec![LineDraft {
                kind: if product_id.is_some() {
                    LineKind::Item
                } else {
                    LineKind::Free
                },
                product_id,
                description: description.into(),
                quantity: Decimal::from(quantity),
                unit: unit.into(),
                unit_price: price,
                tax,
            }],
        },
    )
}

/// Fills an empty database with two businesses (one small business), three
/// customers, four products, ten documents in every state spread over
/// several months, and revenue outside PsyBil for the small business.
pub fn seed(tx: &Transaction, clock: &dyn Clock) -> Result<()> {
    let business = |name: &str, small_business: bool| NewBusiness {
        name: name.into(),
        street: "Musterstraße 1".into(),
        zip: "12345".into(),
        city: "Musterstadt".into(),
        tax_number: "12/345/67890".into(),
        small_business,
        ..NewBusiness::default()
    };
    let (band_business, band) = create_business(tx, clock, business("Beispiel GbR", true))?;
    let small = TaxChoice::Fixed(TaxCategory::SmallBusiness);
    let regular = TaxChoice::Fixed(TaxCategory::Standard {
        rate: Decimal::from(19),
    });
    let (_, studio) = create_business(tx, clock, business("Muster Studio", false))?;
    for (brand_id, color) in [(band, "#8839ef"), (studio, "#1e66f5")] {
        let mut brand = get_brand(tx, brand_id)?;
        brand.logo = Some(LOGO.to_vec());
        brand.logo_mime = Some("image/png".into());
        brand.color = color.into();
        brand.bank_name = "Bank Beispiel".into();
        brand.iban = "DE00 0000 0000 0000 0000 00".into();
        brand.email = "post@beispiel.example".into();
        update_brand(tx, clock, &brand)?;
    }

    let club = customer(tx, clock, "Club Beispiel", CustomerKind::Business)?;
    let festival = customer(tx, clock, "Festival Muster", CustomerKind::Business)?;
    let fan = customer(tx, clock, "Kunde A", CustomerKind::Private)?;

    let d = Decimal::from;
    let shirt = product(
        tx,
        clock,
        band,
        "Shirt",
        ProductKind::Goods,
        "Stk",
        d(20),
        Some(d(20)),
        small.clone(),
    )?;
    let cd = product(
        tx,
        clock,
        band,
        "CD",
        ProductKind::Goods,
        "Stk",
        d(12),
        Some(d(50)),
        small.clone(),
    )?;
    product(
        tx,
        clock,
        studio,
        "Pauschale",
        ProductKind::Service,
        "Pauschal",
        d(250),
        None,
        regular.clone(),
    )?;
    product(
        tx,
        clock,
        studio,
        "Stunde Technik",
        ProductKind::Service,
        "Std.",
        d(45),
        None,
        regular.clone(),
    )?;

    let issue = |tx: &Transaction, id: Uuid, paid: bool| {
        issue_invoice(
            tx,
            clock,
            id,
            IssueOptions {
                payment_received: paid,
            },
        )
    };
    // A draft.
    draft(
        tx,
        clock,
        band,
        fan,
        0,
        ("Shirt", Some(shirt), 2, "Stk", d(20)),
        small.clone(),
    )?;
    // Open and overdue.
    let overdue = draft(
        tx,
        clock,
        studio,
        club,
        40,
        ("Auftritt", None, 1, "Pauschal", d(400)),
        regular.clone(),
    )?;
    issue(tx, overdue, false)?;
    // Partly paid.
    let partly = draft(
        tx,
        clock,
        studio,
        festival,
        10,
        ("Stunde Technik", None, 6, "Std.", d(45)),
        regular.clone(),
    )?;
    issue(tx, partly, false)?;
    add_payment(
        tx,
        clock,
        partly,
        Cents(10000),
        clock.today(),
        PaymentMethod::Transfer,
        "",
    )?;
    // Paid gigs in earlier months, for the receipts per month; issued
    // oldest first, so the numbers follow the dates.
    for (days_ago, price) in [(120, 420), (80, 280), (45, 350)] {
        let gig = draft(
            tx,
            clock,
            band,
            club,
            days_ago,
            ("Auftritt", None, 1, "Pauschal", d(price)),
            small.clone(),
        )?;
        issue(tx, gig, true)?;
    }
    // Cancelled, with its cancellation.
    let cancelled = draft(
        tx,
        clock,
        band,
        club,
        20,
        ("Auftritt", None, 1, "Pauschal", d(300)),
        small.clone(),
    )?;
    issue(tx, cancelled, false)?;
    cancel_invoice(tx, clock, cancelled)?;
    // Paid.
    let paid = draft(
        tx,
        clock,
        band,
        fan,
        5,
        ("CD", Some(cd), 1, "Stk", d(12)),
        small.clone(),
    )?;
    issue(tx, paid, true)?;
    // Open, not due yet.
    let open = draft(
        tx,
        clock,
        studio,
        festival,
        3,
        ("Pauschale", None, 1, "Pauschal", d(250)),
        regular.clone(),
    )?;
    issue(tx, open, false)?;
    // Merch table and platform payouts, entered by hand.
    let year = chrono::Datelike::year(&clock.today());
    crate::overview::set_outside_revenue(
        tx,
        clock,
        band_business,
        year - 1,
        Cents(1_840_000),
        "Merchstand und Plattformen",
    )?;
    crate::overview::set_outside_revenue(
        tx,
        clock,
        band_business,
        year,
        Cents(215_000),
        "Merchstand",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::listing::{InvoiceFilter, list_invoices};
    use crate::masterdata::{list_businesses, list_customers, list_products};
    use crate::payments::StatusKind;
    use crate::testutil::{clock, d};

    #[test]
    fn demo_seed_is_neutral_and_complete() {
        let mut db = Db::open_in_memory().unwrap();
        db.write(|tx| seed(tx, &clock())).unwrap();
        let conn = db.read();
        let businesses: Vec<String> = list_businesses(conn, true)
            .unwrap()
            .into_iter()
            .map(|b| b.name)
            .collect();
        assert_eq!(businesses.len(), 2);
        assert!(businesses.contains(&"Beispiel GbR".to_string()));
        assert!(businesses.contains(&"Muster Studio".to_string()));
        assert_eq!(list_customers(conn, true).unwrap().len(), 3);
        let products: usize = list_businesses(conn, true)
            .unwrap()
            .iter()
            .flat_map(|b| crate::masterdata::list_brands(conn, b.id).unwrap())
            .map(|brand| list_products(conn, brand.id, true).unwrap().len())
            .sum();
        assert_eq!(products, 4);
        let rows = list_invoices(conn, &InvoiceFilter::default(), d(2026, 11, 18)).unwrap();
        assert_eq!(rows.len(), 10);
        assert!(
            rows.iter()
                .any(|r| r.status.kind == StatusKind::Open && !r.status.overdue),
            "a plain open invoice"
        );
        let band = list_businesses(conn, true)
            .unwrap()
            .into_iter()
            .find(|b| b.small_business)
            .unwrap();
        for brand in crate::masterdata::list_brands(conn, band.id).unwrap() {
            for p in list_products(conn, brand.id, true).unwrap() {
                assert_eq!(
                    p.tax,
                    TaxChoice::Fixed(TaxCategory::SmallBusiness),
                    "{}",
                    p.name
                );
            }
        }
        let meter = &crate::overview::overview(conn, Some(band.id), d(2026, 11, 18))
            .unwrap()
            .meters[0];
        assert!(meter.previous.0 >= 1_840_000);
        assert!(meter.current.0 >= 215_000);
        let kinds: Vec<StatusKind> = rows.iter().map(|r| r.status.kind).collect();
        for kind in [
            StatusKind::Draft,
            StatusKind::Open,
            StatusKind::PartlyPaid,
            StatusKind::Paid,
            StatusKind::Cancelled,
        ] {
            assert!(kinds.contains(&kind), "{kind:?} missing");
        }
        assert!(rows.iter().any(|r| r.status.overdue), "an overdue invoice");
    }
}
