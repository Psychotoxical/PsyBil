//! Carrying out an import plan. Everything happens in the caller's
//! transaction: either all of it is taken over or nothing.

use std::collections::HashMap;

use chrono::NaiveDate;
use rusqlite::{OptionalExtension, Transaction, params};
use rust_decimal::Decimal;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::audit;
use crate::calc::{LineKind, PriceMode};
use crate::dates::{Clock, ServiceDate};
use crate::invoice::{DraftInput, LineDraft, create_draft, load_invoice};
use crate::issue::{build_snapshot, insert_payment};
use crate::legacy::plan::{ImportPlan, LegacyClass, ReviewReason, legacy_customer_key};
use crate::legacy::read::{LegacyData, LegacyInvoice, LegacySeller};
use crate::masterdata::{
    Brand, CustomerKind, NewBrand, NewBusiness, NewCustomer, NewProduct, ProductKind, create_brand,
    create_business, create_customer, create_product, get_brand, list_brands, update_brand,
};
use crate::merge::quarantine;
use crate::money::Cents;
use crate::tax::{TaxCategory, TaxChoice};
use crate::{Error, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ImportReport {
    pub businesses: usize,
    pub customers: usize,
    pub products: usize,
    pub drafts: usize,
    pub issued: usize,
    pub payments: usize,
    pub quarantined: usize,
    pub needs_review: usize,
    /// Invoices and drafts that an earlier import already took over.
    pub already_present: usize,
}

pub(crate) fn cents(value: f64) -> Cents {
    Cents((value * 100.0).round() as i64)
}

fn decimal(value: f64) -> Decimal {
    Decimal::from_f64(value)
        .unwrap_or_default()
        .round_dp(4)
        .normalize()
}

fn country_code(country: &str) -> String {
    match country.trim().to_lowercase().as_str() {
        "" | "deutschland" | "germany" | "de" => "DE".into(),
        _ => country.trim().to_string(),
    }
}

/// "14 Tage netto" → 14, "Sofort fällig" → 0, otherwise 14.
fn payment_days(terms: &str) -> u32 {
    if terms.to_lowercase().contains("sofort") {
        return 0;
    }
    let digits: String = terms
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().unwrap_or(14)
}

fn tax_choice(rate: f64, small_business: bool) -> TaxChoice {
    if small_business {
        TaxChoice::Fixed(TaxCategory::SmallBusiness)
    } else if rate == 0.0 {
        TaxChoice::Fixed(TaxCategory::Exempt {
            reason: String::new(),
        })
    } else {
        TaxChoice::Fixed(TaxCategory::Standard {
            rate: decimal(rate),
        })
    }
}

/// Totals of an imported invoice from what VibeBill stored. Line amounts
/// and taxes are rounded per line, as VibeBill did; any rest to the stored
/// net and tax goes to the largest group, so the frozen document (and a
/// later cancellation of it) adds up to what the customer was charged.
fn legacy_totals(lines: &[LineDraft], net: Cents, tax: Cents, gross: Cents) -> crate::calc::Totals {
    use rust_decimal::RoundingStrategy::MidpointAwayFromZero;
    let to_cents = |d: Decimal| {
        Cents(
            (d.round_dp_with_strategy(2, MidpointAwayFromZero) * Decimal::ONE_HUNDRED)
                .to_i64()
                .unwrap_or(0),
        )
    };
    let mut line_amounts = Vec::with_capacity(lines.len());
    let mut groups: Vec<crate::calc::TaxGroup> = Vec::new();
    for line in lines {
        let amount = line.quantity * line.unit_price;
        line_amounts.push(to_cents(amount));
        let TaxChoice::Fixed(category) = &line.tax else {
            continue;
        };
        let line_tax = to_cents(amount * category.rate() / Decimal::ONE_HUNDRED);
        match groups.iter_mut().find(|g| &g.category == category) {
            Some(g) => {
                g.base = Cents(g.base.0 + to_cents(amount).0);
                g.tax = Cents(g.tax.0 + line_tax.0);
            }
            None => groups.push(crate::calc::TaxGroup {
                category: category.clone(),
                base: to_cents(amount),
                tax: line_tax,
                gross: Cents::ZERO,
            }),
        }
    }
    let largest = |groups: &[crate::calc::TaxGroup], taxed: bool| {
        groups
            .iter()
            .enumerate()
            .filter(|(_, g)| !taxed || !g.category.rate().is_zero())
            .max_by_key(|(_, g)| g.base.0.abs())
            .map(|(i, _)| i)
    };
    let net_rest = net.0 - groups.iter().map(|g| g.base.0).sum::<i64>();
    if net_rest != 0
        && let Some(i) = largest(&groups, false)
    {
        groups[i].base = Cents(groups[i].base.0 + net_rest);
    }
    let tax_rest = tax.0 - groups.iter().map(|g| g.tax.0).sum::<i64>();
    if tax_rest != 0
        && let Some(i) = largest(&groups, true).or_else(|| largest(&groups, false))
    {
        groups[i].tax = Cents(groups[i].tax.0 + tax_rest);
    }
    for g in &mut groups {
        g.gross = Cents(g.base.0 + g.tax.0);
    }
    crate::calc::Totals {
        line_amounts,
        groups,
        net,
        tax,
        gross,
    }
}

fn payment_method(old: &str) -> &'static str {
    match old {
        "bank" => "transfer",
        "cash" => "cash",
        "paypal" => "paypal",
        _ => "other",
    }
}

fn parse_date(text: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d")
        .map_err(|_| Error::Import(format!("unreadable date {text:?}")))
}

fn fill_brand(brand: &mut Brand, seller: &LegacySeller) {
    brand.name = seller.name.clone();
    if let Some((bytes, mime)) = &seller.logo {
        brand.logo = Some(bytes.clone());
        brand.logo_mime = Some(mime.clone());
    }
    brand.color = seller.color.clone();
    brand.phone = seller.phone.clone();
    brand.email = seller.email.clone();
    brand.website = seller.website.clone();
    brand.bank_name = seller.bank_name.clone();
    brand.iban = seller.bank_iban.clone();
    brand.bic = seller.bank_bic.clone();
    brand.pdf_template = seller.pdf_template.clone();
    brand.invoice_prefix = seller.invoice_prefix.clone();
    brand.payment_days = payment_days(&seller.default_payment_terms);
    brand.outro_text = seller.default_note.clone();
}

struct SellerTarget {
    business: Uuid,
    brand: Uuid,
    prefix: String,
    small_business: bool,
}

/// Takes over `data` as `plan` says.
pub fn apply_import(
    tx: &Transaction,
    clock: &dyn Clock,
    data: &LegacyData,
    plan: &ImportPlan,
) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    let seller = |id: i64| {
        data.sellers
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| Error::Import(format!("seller {id} is not in the old database")))
    };

    // Businesses and their brands.
    let mut targets: HashMap<i64, SellerTarget> = HashMap::new();
    for planned in &plan.businesses {
        let first = seller(
            *planned
                .sellers
                .first()
                .ok_or_else(|| Error::Import("business without seller".into()))?,
        )?;
        let (business, default_brand) = match planned.existing {
            // Taken over before: its brands stay as they are now.
            Some(existing) => (existing, None),
            None => {
                let (business, brand) = create_business(
                    tx,
                    clock,
                    NewBusiness {
                        name: planned.name.clone(),
                        street: first.street.clone(),
                        zip: first.zip.clone(),
                        city: first.city.clone(),
                        country: country_code(&first.country),
                        tax_number: first.tax_id.clone(),
                        vat_id: first.vat_id.clone(),
                        small_business: planned.small_business,
                        currency: first.currency.clone(),
                        ..NewBusiness::default()
                    },
                )?;
                report.businesses += 1;
                (business, Some(brand))
            }
        };
        for (index, seller_id) in planned.sellers.iter().enumerate() {
            let old = seller(*seller_id)?;
            let known = if default_brand.is_none() {
                list_brands(tx, business)?
                    .into_iter()
                    .find(|b| b.name == old.name)
            } else {
                None
            };
            if let Some(brand) = known {
                targets.insert(
                    *seller_id,
                    SellerTarget {
                        business,
                        brand: brand.id,
                        prefix: old.invoice_prefix.clone(),
                        small_business: planned.small_business,
                    },
                );
                continue;
            }
            let brand_id = if let (0, Some(default_brand)) = (index, default_brand) {
                default_brand
            } else {
                create_brand(
                    tx,
                    clock,
                    NewBrand {
                        business_id: business,
                        name: old.name.clone(),
                        ..NewBrand::default()
                    },
                )?
            };
            let mut brand = get_brand(tx, brand_id)?;
            fill_brand(&mut brand, old);
            update_brand(tx, clock, &brand)?;
            targets.insert(
                *seller_id,
                SellerTarget {
                    business,
                    brand: brand_id,
                    prefix: old.invoice_prefix.clone(),
                    small_business: planned.small_business,
                },
            );
        }
    }
    let target = |seller_id: i64| {
        targets
            .get(&seller_id)
            .ok_or_else(|| Error::Import(format!("seller {seller_id} is not part of the plan")))
    };

    // Customers: reuse matches and duplicates within the old data.
    let matches: HashMap<i64, Option<Uuid>> = plan.customer_matches.iter().copied().collect();
    let mut by_key: HashMap<String, Uuid> = HashMap::new();
    let mut customers: HashMap<i64, Uuid> = HashMap::new();
    for old in &data.customers {
        let key = legacy_customer_key(old);
        let id = if let Some(Some(existing)) = matches.get(&old.id) {
            *existing
        } else if let Some(existing) = by_key.get(&key) {
            *existing
        } else {
            report.customers += 1;
            create_customer(
                tx,
                clock,
                NewCustomer {
                    name: old.name.clone(),
                    street: old.street.clone(),
                    zip: old.zip.clone(),
                    city: old.city.clone(),
                    country: country_code(&old.country),
                    kind: CustomerKind::Unknown,
                    email: old.email.clone(),
                    phone: old.phone.clone(),
                    notes: old.notes.clone(),
                    ..NewCustomer::default()
                },
            )?
        };
        by_key.insert(key, id);
        customers.insert(old.id, id);
    }

    // Products.
    let product_matches: HashMap<i64, Option<Uuid>> =
        plan.product_matches.iter().copied().collect();
    let mut products: HashMap<i64, Uuid> = HashMap::new();
    for old in &data.products {
        let Ok(t) = target(old.seller_id) else {
            continue;
        };
        let id = if let Some(Some(existing)) = product_matches.get(&old.id) {
            *existing
        } else {
            report.products += 1;
            let goods = old.kind != "service";
            create_product(
                tx,
                clock,
                NewProduct {
                    brand_id: t.brand,
                    name: old.name.clone(),
                    description: old.description.clone(),
                    kind: if goods {
                        ProductKind::Goods
                    } else {
                        ProductKind::Service
                    },
                    unit: old.unit.clone(),
                    price: decimal(old.price_net),
                    price_mode: PriceMode::Net,
                    tax: tax_choice(old.tax_rate, t.small_business),
                    stock: goods.then(|| Decimal::from(old.stock)),
                    active: old.active,
                },
            )?
        };
        products.insert(old.id, id);
    }

    // Invoices.
    for planned in &plan.invoices {
        let old = data
            .invoices
            .iter()
            .find(|i| i.id == planned.legacy_id)
            .ok_or_else(|| {
                Error::Import(format!(
                    "invoice {} is not in the old database",
                    planned.legacy_id
                ))
            })?;
        let t = target(old.seller_id)?;
        import_invoice(
            tx,
            clock,
            &mut report,
            data,
            old,
            planned.class,
            &planned.review,
            t,
            &customers,
            &products,
        )?;
    }

    // Counters continue after the highest taken-over number.
    for t in targets.values() {
        let mut stmt = tx
            .prepare("SELECT number FROM invoices WHERE business_id = ?1 AND number IS NOT NULL")?;
        let numbers = stmt
            .query_map([t.business.to_string()], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let head = format!("{}-", t.prefix);
        for number in numbers {
            let Some(rest) = number.strip_prefix(&head) else {
                continue;
            };
            let Some((year, n)) = rest.split_once('-') else {
                continue;
            };
            let (Ok(year), Ok(n)) = (year.parse::<i32>(), n.parse::<u32>()) else {
                continue;
            };
            tx.execute(
                "INSERT INTO number_counters (business_id, year, next) VALUES (?1, ?2, ?3)
                 ON CONFLICT (business_id, year) DO UPDATE SET next = MAX(next, excluded.next)",
                params![t.business.to_string(), year, n + 1],
            )?;
        }
    }

    audit::log(
        tx,
        clock,
        "database",
        Uuid::nil(),
        "import",
        serde_json::to_value(&report)?,
    )?;
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn import_invoice(
    tx: &Transaction,
    clock: &dyn Clock,
    report: &mut ImportReport,
    data: &LegacyData,
    old: &LegacyInvoice,
    class: LegacyClass,
    review: &[ReviewReason],
    t: &SellerTarget,
    customers: &HashMap<i64, Uuid>,
    products: &HashMap<i64, Uuid>,
) -> Result<()> {
    // A document that was sent with VAT keeps its rates, even for a small
    // business: it has to show what the customer received. Drafts are
    // issued later and follow the business.
    let charged_tax = cents(old.total_tax) != Cents::ZERO;
    let small = t.small_business && (class == LegacyClass::NeverSent || !charged_tax);
    let mut lines: Vec<LineDraft> = old
        .items
        .iter()
        .map(|item| {
            let product_id = item.product_id.and_then(|p| products.get(&p).copied());
            LineDraft {
                kind: if product_id.is_some() {
                    LineKind::Item
                } else {
                    LineKind::Free
                },
                product_id,
                description: item.description.clone(),
                quantity: decimal(item.quantity),
                unit: item.unit.clone(),
                unit_price: decimal(item.price_net),
                tax: tax_choice(item.tax_rate, small),
            }
        })
        .collect();
    if old.shipping_net > 0.0 {
        lines.push(LineDraft {
            kind: LineKind::Shipping,
            product_id: None,
            description: "Versand".into(),
            quantity: Decimal::ONE,
            unit: String::new(),
            unit_price: decimal(old.shipping_net),
            tax: tax_choice(old.shipping_tax_rate, small),
        });
    }
    let invoice_date = parse_date(&old.date)?;
    let due = parse_date(&old.due_date).unwrap_or(invoice_date);
    let days = (due - invoice_date).num_days().max(0) as u32;
    let mut notes = old.notes.clone();
    if class == LegacyClass::NeverSent {
        notes = format!("{notes}\nAlte Nummer: {}", old.invoice_number)
            .trim()
            .to_string();
    }
    let input = DraftInput {
        brand_id: t.brand,
        customer_id: customers.get(&old.customer_id).copied(),
        invoice_date,
        service_date: ServiceDate::SameAsInvoice,
        payment_days: days,
        price_mode: PriceMode::Net,
        language: "de".into(),
        intro_text: String::new(),
        outro_text: String::new(),
        notes,
        lines,
    };
    if class == LegacyClass::NeverSent {
        let present: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM invoices WHERE business_id = ?1 AND state = 'draft'
                           AND instr(notes, ?2) > 0)",
            params![
                t.business.to_string(),
                format!("Alte Nummer: {}", old.invoice_number)
            ],
            |r| r.get(0),
        )?;
        if present {
            report.already_present += 1;
        } else {
            create_draft(tx, clock, input)?;
            report.drafts += 1;
        }
        return Ok(());
    }

    let taken: Option<(bool, Option<i64>)> = tx
        .query_row(
            "SELECT imported, gross_cents FROM invoices WHERE business_id = ?1 AND number = ?2",
            params![t.business.to_string(), old.invoice_number],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if taken == Some((true, Some(cents(old.total_gross).0))) {
        // Taken over before: the same document, not a second one.
        report.already_present += 1;
        return Ok(());
    }
    if taken.is_some() {
        quarantine(
            tx,
            clock,
            "legacy_invoice",
            "number_collision",
            serde_json::json!({ "invoice": old, "payments": data.payments.iter().filter(|p| p.invoice_id == old.id).collect::<Vec<_>>() }),
        )?;
        report.quarantined += 1;
        return Ok(());
    }

    let id = create_draft(tx, clock, input)?;
    let needs_review = (t.small_business && charged_tax)
        || review
            .iter()
            .any(|r| !(t.small_business && *r == ReviewReason::ZeroRateWithoutReason));
    let (net, tax, gross) = (
        cents(old.total_net),
        cents(old.total_tax),
        cents(old.total_gross),
    );
    tx.execute(
        "UPDATE invoices SET imported = 1, needs_review = ?2, legacy_cancelled = ?3, number = ?4,
            net_cents = ?5, tax_cents = ?6, gross_cents = ?7
         WHERE id = ?1",
        params![
            id.to_string(),
            needs_review,
            class == LegacyClass::Cancelled,
            old.invoice_number,
            net.0,
            tax.0,
            gross.0
        ],
    )?;
    tx.execute(
        "UPDATE invoices SET state = 'issued' WHERE id = ?1",
        [id.to_string()],
    )?;

    let invoice = load_invoice(tx, id)?;
    let totals = legacy_totals(&invoice.lines, net, tax, gross);
    let snapshot = build_snapshot(tx, &invoice, &old.invoice_number, &totals, true)?;
    tx.execute(
        "INSERT INTO invoice_snapshots (invoice_id, snapshot, created_at) VALUES (?1, ?2, ?3)",
        params![
            id.to_string(),
            snapshot.to_string(),
            crate::dates::timestamp(clock)
        ],
    )?;
    crate::pdf::archive::archive_pdf(tx, id)?;
    report.issued += 1;
    if needs_review {
        report.needs_review += 1;
    }

    let mut paid = Cents::ZERO;
    for payment in data.payments.iter().filter(|p| p.invoice_id == old.id) {
        let amount = cents(payment.amount);
        insert_payment(
            tx,
            clock,
            id,
            amount,
            parse_date(&payment.date)?,
            payment_method(&payment.method),
            &payment.notes,
            None,
        )?;
        paid = paid.checked_add(amount)?;
        report.payments += 1;
    }
    if class == LegacyClass::SentPaid && paid < gross {
        insert_payment(
            tx,
            clock,
            id,
            gross.checked_sub(paid)?,
            invoice_date,
            "legacy",
            "aus VibeBill übernommen",
            None,
        )?;
        report.payments += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::invoice::{InvoiceState, create_draft, load_invoice};
    use crate::issue::{IssueOptions, issue_invoice};
    use crate::legacy::plan::plan_import;
    use crate::legacy::read::read_legacy;
    use crate::legacy::testdata::{early_db, full_db};
    use crate::masterdata::{list_brands, list_businesses, list_customers};
    use crate::payments::{PaymentMethod, StatusKind, invoice_status, list_payments};
    use crate::testutil::{clock, d, sample_draft};

    struct Imported {
        db: Db,
        report: ImportReport,
    }

    fn import_into(db: &mut Db, build: fn(&std::path::Path), small_business: bool) -> ImportReport {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vibebill.db");
        build(&path);
        let data = read_legacy(&path, dir.path()).unwrap();
        let mut plan = plan_import(db.read(), &data).unwrap();
        for b in &mut plan.businesses {
            b.small_business = small_business;
        }
        db.write(|tx| apply_import(tx, &clock(), &data, &plan))
            .unwrap()
    }

    fn imported() -> Imported {
        let mut db = Db::open_in_memory().unwrap();
        let report = import_into(&mut db, full_db, true);
        Imported { db, report }
    }

    fn by_number(db: &Db, number: &str) -> crate::invoice::Invoice {
        let id: String = db
            .read()
            .query_row("SELECT id FROM invoices WHERE number = ?1", [number], |r| {
                r.get(0)
            })
            .unwrap();
        load_invoice(db.read(), id.parse().unwrap()).unwrap()
    }

    #[test]
    fn imports_everything_once() {
        let i = imported();
        let r = &i.report;
        assert_eq!((r.businesses, r.customers, r.products), (2, 2, 2));
        assert_eq!((r.drafts, r.issued, r.quarantined), (1, 4, 1));
        assert_eq!(r.payments, 2);
        assert!(r.needs_review >= 1);
    }

    #[test]
    fn never_sent_becomes_draft_with_old_number_note() {
        let i = imported();
        let id: String =
            i.db.read()
                .query_row(
                    "SELECT id FROM invoices WHERE notes LIKE '%RE-2026-0003%'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
        let invoice = load_invoice(i.db.read(), id.parse().unwrap()).unwrap();
        assert_eq!(invoice.state, InvoiceState::Draft);
        assert_eq!(invoice.number, None);
        assert!(invoice.notes.contains("Alte Nummer: RE-2026-0003"));
    }

    #[test]
    fn issued_keeps_stored_totals_and_number() {
        let i = imported();
        let invoice = by_number(&i.db, "RE-2026-0002");
        assert_eq!(invoice.state, InvoiceState::Issued);
        assert_eq!(
            (invoice.net, invoice.tax, invoice.gross),
            (Some(Cents(3852)), Some(Cents(639)), Some(Cents(4491)))
        );
        assert!(invoice.imported);
        assert!(invoice.needs_review);
        assert_eq!(invoice.lines.len(), 2);
        assert_eq!(invoice.lines[1].kind, crate::calc::LineKind::Shipping);
        let snapshot = crate::issue::load_snapshot(i.db.read(), invoice.id).unwrap();
        assert_eq!(snapshot["imported"], true);
    }

    #[test]
    fn cancelled_shows_status_cancelled() {
        let i = imported();
        let invoice = by_number(&i.db, "RE-2026-0004");
        assert!(invoice.legacy_cancelled);
        assert_eq!(
            invoice_status(i.db.read(), invoice.id, d(2026, 11, 18))
                .unwrap()
                .kind,
            StatusKind::Cancelled
        );
    }

    #[test]
    fn paid_without_payments_gets_legacy_payment() {
        let i = imported();
        let invoice = by_number(&i.db, "RE-2026-0001");
        let payments = list_payments(i.db.read(), invoice.id).unwrap();
        assert_eq!(payments.len(), 1);
        assert_eq!(
            (payments[0].amount, payments[0].method, payments[0].date),
            (Cents(34000), PaymentMethod::Legacy, d(2026, 3, 10))
        );
        assert_eq!(
            invoice_status(i.db.read(), invoice.id, d(2026, 11, 18))
                .unwrap()
                .kind,
            StatusKind::Paid
        );
        let partial = by_number(&i.db, "RE-2026-0002");
        assert_eq!(
            list_payments(i.db.read(), partial.id).unwrap()[0].method,
            PaymentMethod::Transfer
        );
    }

    #[test]
    fn counter_continues_after_highest_number() {
        let mut i = imported();
        let band = list_businesses(i.db.read(), false)
            .unwrap()
            .into_iter()
            .find(|b| b.name == "Die Band")
            .unwrap();
        let brand = list_brands(i.db.read(), band.id).unwrap()[0].id;
        let customer = list_customers(i.db.read(), false).unwrap()[0].id;
        let id =
            i.db.write(|tx| create_draft(tx, &clock(), sample_draft(brand, Some(customer))))
                .unwrap();
        let issued =
            i.db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()))
                .unwrap();
        assert_eq!(issued.number, "RE-2026-0005");
    }

    #[test]
    fn second_import_reuses_customers() {
        let mut db = Db::open_in_memory().unwrap();
        import_into(&mut db, full_db, true);
        let report = import_into(&mut db, early_db, true);
        assert_eq!(report.customers, 0);
        let clubs = list_customers(db.read(), true)
            .unwrap()
            .into_iter()
            .filter(|c| c.name == "Club Beispiel")
            .count();
        assert_eq!(clubs, 1);
    }

    fn line(price: rust_decimal::Decimal) -> LineDraft {
        LineDraft {
            kind: LineKind::Free,
            product_id: None,
            description: "Posten".into(),
            quantity: Decimal::ONE,
            unit: String::new(),
            unit_price: price,
            tax: TaxChoice::Fixed(TaxCategory::Standard {
                rate: rust_decimal::dec!(19),
            }),
        }
    }

    #[test]
    fn legacy_totals_round_per_line() {
        let lines = vec![
            line(rust_decimal::dec!(0.33)),
            line(rust_decimal::dec!(0.33)),
            line(rust_decimal::dec!(0.33)),
        ];
        let totals = legacy_totals(&lines, Cents(99), Cents(18), Cents(117));
        assert_eq!(totals.groups.len(), 1);
        assert_eq!(
            (totals.groups[0].base, totals.groups[0].tax),
            (Cents(99), Cents(18))
        );
        assert_eq!(totals.line_amounts, vec![Cents(33), Cents(33), Cents(33)]);
    }

    fn sums_add_up(totals: &crate::calc::Totals) {
        let base: i64 = totals.groups.iter().map(|g| g.base.0).sum();
        let tax: i64 = totals.groups.iter().map(|g| g.tax.0).sum();
        assert_eq!(base, totals.net.0, "bases vs net");
        assert_eq!(tax, totals.tax.0, "group tax vs tax");
        assert_eq!(
            totals.net.0 + totals.tax.0,
            totals.gross.0,
            "net + tax vs gross"
        );
    }

    #[test]
    fn imported_totals_add_up() {
        let i = imported();
        let invoice = by_number(&i.db, "RE-2026-0002");
        let snapshot = crate::issue::load_snapshot(i.db.read(), invoice.id).unwrap();
        let totals: crate::calc::Totals =
            serde_json::from_value(snapshot["totals"].clone()).unwrap();
        assert_eq!(totals.tax, Cents(639));
        sums_add_up(&totals);
    }

    #[test]
    fn storno_of_imported_keeps_stored_vat() {
        let mut i = imported();
        let invoice = by_number(&i.db, "RE-2026-0002");
        let storno =
            i.db.write(|tx| crate::storno::cancel_invoice(tx, &clock(), invoice.id))
                .unwrap();
        let snapshot = crate::issue::load_snapshot(i.db.read(), storno).unwrap();
        let totals: crate::calc::Totals =
            serde_json::from_value(snapshot["totals"].clone()).unwrap();
        assert_eq!((totals.tax, totals.gross), (Cents(-639), Cents(-4491)));
        sums_add_up(&totals);
    }

    #[test]
    fn import_archives_marked_pdf() {
        let i = imported();
        let invoice = by_number(&i.db, "RE-2026-0002");
        let stored: Option<Vec<u8>> =
            i.db.read()
                .query_row(
                    "SELECT pdf FROM invoice_snapshots WHERE invoice_id = ?1",
                    [invoice.id.to_string()],
                    |r| r.get(0),
                )
                .unwrap();
        let pdf = crate::pdf::invoice_pdf(i.db.read(), invoice.id).unwrap();
        assert_eq!(stored.as_deref(), Some(pdf.as_slice()));
        let text = pdf_extract::extract_text_from_mem(&pdf).unwrap();
        assert!(text.contains("Nachträglich erzeugt aus übernommenen Daten."));
    }

    #[test]
    fn legacy_cancelled_cannot_be_cancelled_again() {
        let mut i = imported();
        let invoice = by_number(&i.db, "RE-2026-0004");
        assert!(matches!(
            i.db.write(|tx| crate::storno::cancel_invoice(tx, &clock(), invoice.id)),
            Err(Error::NotCancellable)
        ));
    }

    #[test]
    fn small_business_invoice_with_tax_keeps_rate_and_needs_review() {
        let i = imported();
        let invoice = by_number(&i.db, "SB-2026-0001");
        assert_eq!(invoice.tax, Some(Cents(3420)));
        assert!(invoice.needs_review);
        assert!(
            invoice
                .lines
                .iter()
                .all(|l| matches!(l.tax, TaxChoice::Fixed(TaxCategory::Standard { .. })))
        );
    }

    #[test]
    fn importing_the_same_database_twice_adds_nothing() {
        let mut db = Db::open_in_memory().unwrap();
        import_into(&mut db, full_db, true);
        let count = |db: &Db| -> i64 {
            db.read()
                .query_row("SELECT COUNT(*) FROM invoices", [], |r| r.get(0))
                .unwrap()
        };
        let before = count(&db);
        let again = import_into(&mut db, full_db, true);
        assert_eq!((again.businesses, again.drafts, again.issued), (0, 0, 0));
        assert_eq!(list_businesses(db.read(), true).unwrap().len(), 2);
        assert_eq!(count(&db), before);
    }

    #[test]
    fn failure_rolls_back_everything() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vibebill.db");
        full_db(&path);
        let data = read_legacy(&path, dir.path()).unwrap();
        let mut db = Db::open_in_memory().unwrap();
        let mut plan = plan_import(db.read(), &data).unwrap();
        plan.businesses[0].sellers = vec![99];
        assert!(
            db.write(|tx| apply_import(tx, &clock(), &data, &plan))
                .is_err()
        );
        assert!(list_businesses(db.read(), true).unwrap().is_empty());
    }

    #[test]
    fn logo_is_imported() {
        let i = imported();
        let band = list_businesses(i.db.read(), false)
            .unwrap()
            .into_iter()
            .find(|b| b.name == "Die Band")
            .unwrap();
        let brand = &list_brands(i.db.read(), band.id).unwrap()[0];
        assert_eq!(brand.logo_mime.as_deref(), Some("image/png"));
        assert!(brand.logo.as_ref().is_some_and(|l| l.len() > 8));
        assert_eq!(brand.invoice_prefix, "RE");
        assert_eq!(brand.pdf_template, "modern");
    }
}
