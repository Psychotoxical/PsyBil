//! The proposal shown before an import: how each old invoice is classified,
//! which customers already exist, and what needs a look.

use std::collections::HashMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Result;
use crate::db::uuid_col;
use crate::legacy::read::{LegacyCustomer, LegacyData, LegacyInvoice};

/// How an old invoice is taken over. VibeBill only stored a hand-set
/// status, so the user confirms this per invoice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LegacyClass {
    /// Never sent: becomes a draft without number.
    NeverSent,
    SentOpen,
    SentPaid,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewReason {
    /// Stored net + tax is not the stored gross.
    TotalsMismatch,
    /// Shipping with a tax that does not match the stored totals (the old
    /// 0 % shipping bug).
    ShippingTaxUnclear,
    /// A line at 0 % without a reason.
    ZeroRateWithoutReason,
    MissingCustomer,
    /// The number appears twice for the same sender.
    DuplicateNumber,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedBusiness {
    /// Old sellers that become brands of this business.
    pub sellers: Vec<i64>,
    pub name: String,
    pub small_business: bool,
    /// A business taken over by an earlier import (same name and tax
    /// number); the import adds to it instead of creating a second one.
    #[serde(default)]
    pub existing: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedInvoice {
    pub legacy_id: i64,
    pub number: String,
    pub class: LegacyClass,
    pub review: Vec<ReviewReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportPlan {
    pub businesses: Vec<PlannedBusiness>,
    pub invoices: Vec<PlannedInvoice>,
    /// Old customer id → an existing PsyBil customer to reuse.
    pub customer_matches: Vec<(i64, Option<Uuid>)>,
    /// Old product id → an existing PsyBil product to reuse.
    pub product_matches: Vec<(i64, Option<Uuid>)>,
}

/// Name and address in a form that ignores case and spacing.
pub(crate) fn customer_key(name: &str, street: &str, zip: &str, city: &str) -> String {
    [name, street, zip, city]
        .iter()
        .map(|s| {
            s.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        })
        .collect::<Vec<_>>()
        .join("|")
}

pub(crate) fn legacy_customer_key(c: &LegacyCustomer) -> String {
    customer_key(&c.name, &c.street, &c.zip, &c.city)
}

fn cents(value: f64) -> i64 {
    (value * 100.0).round() as i64
}

/// The old app rounded the tax per line.
fn line_tax_cents(net: f64, rate: f64) -> i64 {
    ((cents(net) as f64) * rate / 100.0).round() as i64
}

fn class_of(status: &str) -> LegacyClass {
    match status {
        "draft" => LegacyClass::NeverSent,
        "paid" => LegacyClass::SentPaid,
        "cancelled" => LegacyClass::Cancelled,
        _ => LegacyClass::SentOpen,
    }
}

fn review_of(
    invoice: &LegacyInvoice,
    data: &LegacyData,
    duplicates: &HashMap<(i64, String), usize>,
) -> Vec<ReviewReason> {
    let mut review = Vec::new();
    if cents(invoice.total_net) + cents(invoice.total_tax) != cents(invoice.total_gross) {
        review.push(ReviewReason::TotalsMismatch);
    }
    if invoice.shipping_net > 0.0 {
        let items: i64 = invoice
            .items
            .iter()
            .map(|i| line_tax_cents(i.quantity * i.price_net, i.tax_rate))
            .sum();
        let expected = items + line_tax_cents(invoice.shipping_net, invoice.shipping_tax_rate);
        if expected != cents(invoice.total_tax) {
            review.push(ReviewReason::ShippingTaxUnclear);
        }
    }
    if invoice.items.iter().any(|i| i.tax_rate == 0.0) {
        review.push(ReviewReason::ZeroRateWithoutReason);
    }
    if !data.customers.iter().any(|c| c.id == invoice.customer_id) {
        review.push(ReviewReason::MissingCustomer);
    }
    if duplicates
        .get(&(invoice.seller_id, invoice.invoice_number.clone()))
        .is_some_and(|n| *n > 1)
    {
        review.push(ReviewReason::DuplicateNumber);
    }
    review
}

/// Proposes how to take over `data` into the database behind `conn`.
pub fn plan_import(conn: &Connection, data: &LegacyData) -> Result<ImportPlan> {
    let known: Vec<(Uuid, String, String, bool)> = {
        let mut stmt =
            conn.prepare("SELECT id, name, tax_number, small_business FROM businesses")?;
        stmt.query_map([], |r| {
            Ok((uuid_col(r, 0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?
    };
    let businesses = data
        .sellers
        .iter()
        .map(|s| {
            let found = known.iter().find(|(_, name, tax, _)| {
                name.trim().eq_ignore_ascii_case(s.name.trim()) && tax.trim() == s.tax_id.trim()
            });
            PlannedBusiness {
                sellers: vec![s.id],
                name: s.name.clone(),
                small_business: found.is_some_and(|f| f.3),
                existing: found.map(|f| f.0),
            }
        })
        .collect();

    let mut duplicates: HashMap<(i64, String), usize> = HashMap::new();
    for invoice in &data.invoices {
        *duplicates
            .entry((invoice.seller_id, invoice.invoice_number.clone()))
            .or_default() += 1;
    }
    let invoices = data
        .invoices
        .iter()
        .map(|i| PlannedInvoice {
            legacy_id: i.id,
            number: i.invoice_number.clone(),
            class: class_of(&i.status),
            review: review_of(i, data, &duplicates),
        })
        .collect();

    let mut existing: HashMap<String, Uuid> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, name, street, zip, city FROM customers")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                uuid_col(r, 0)?,
                customer_key(
                    &r.get::<_, String>(1)?,
                    &r.get::<_, String>(2)?,
                    &r.get::<_, String>(3)?,
                    &r.get::<_, String>(4)?,
                ),
            ))
        })?;
        for row in rows {
            let (id, key) = row?;
            existing.entry(key).or_insert(id);
        }
    }
    let customer_matches = data
        .customers
        .iter()
        .map(|c| (c.id, existing.get(&legacy_customer_key(c)).copied()))
        .collect();

    let mut products: HashMap<String, Uuid> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT p.id, lower(p.name), lower(b.name) FROM products p JOIN brands b ON b.id = p.brand_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                uuid_col(r, 0)?,
                format!("{}|{}", r.get::<_, String>(2)?, r.get::<_, String>(1)?),
            ))
        })?;
        for row in rows {
            let (id, key) = row?;
            products.entry(key).or_insert(id);
        }
    }
    let product_matches = data
        .products
        .iter()
        .map(|p| {
            let seller = data
                .sellers
                .iter()
                .find(|s| s.id == p.seller_id)
                .map(|s| s.name.to_lowercase())
                .unwrap_or_default();
            (
                p.id,
                products
                    .get(&format!("{seller}|{}", p.name.to_lowercase()))
                    .copied(),
            )
        })
        .collect();

    Ok(ImportPlan {
        businesses,
        invoices,
        customer_matches,
        product_matches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::legacy::read::read_legacy;
    use crate::legacy::testdata::full_db;
    use crate::masterdata::create_customer;
    use crate::testutil::{clock, sample_customer};

    fn planned(existing_customer: bool) -> (ImportPlan, Option<Uuid>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vibebill.db");
        full_db(&path);
        let data = read_legacy(&path, dir.path()).unwrap();
        let mut db = Db::open_in_memory().unwrap();
        let existing = existing_customer.then(|| {
            db.write(|tx| create_customer(tx, &clock(), sample_customer()))
                .unwrap()
        });
        (plan_import(db.read(), &data).unwrap(), existing)
    }

    fn invoice(plan: &ImportPlan, id: i64) -> &PlannedInvoice {
        plan.invoices.iter().find(|i| i.legacy_id == id).unwrap()
    }

    #[test]
    fn classes_follow_old_status() {
        let (plan, _) = planned(false);
        assert_eq!(invoice(&plan, 1).class, LegacyClass::SentPaid);
        assert_eq!(invoice(&plan, 2).class, LegacyClass::SentOpen);
        assert_eq!(invoice(&plan, 3).class, LegacyClass::NeverSent);
        assert_eq!(invoice(&plan, 4).class, LegacyClass::Cancelled);
        assert_eq!(invoice(&plan, 6).class, LegacyClass::SentOpen);
        assert_eq!(plan.businesses.len(), 2);
        assert!(
            plan.businesses
                .iter()
                .all(|b| !b.small_business && b.sellers.len() == 1)
        );
    }

    #[test]
    fn detects_duplicate_customer_against_existing() {
        let (plan, existing) = planned(true);
        let matches: Vec<(i64, Option<Uuid>)> = plan.customer_matches.clone();
        assert!(matches.contains(&(2, existing)));
        assert!(matches.contains(&(1, None)));
    }

    #[test]
    fn flags_totals_mismatch() {
        let (plan, _) = planned(false);
        assert!(
            invoice(&plan, 5)
                .review
                .contains(&ReviewReason::TotalsMismatch)
        );
        assert!(
            !invoice(&plan, 6)
                .review
                .contains(&ReviewReason::TotalsMismatch)
        );
    }

    #[test]
    fn flags_zero_percent_shipping_trap() {
        let (plan, _) = planned(false);
        assert!(
            invoice(&plan, 2)
                .review
                .contains(&ReviewReason::ShippingTaxUnclear)
        );
        assert!(
            invoice(&plan, 1)
                .review
                .contains(&ReviewReason::ZeroRateWithoutReason)
        );
    }

    #[test]
    fn flags_duplicate_number() {
        let (plan, _) = planned(false);
        assert!(
            invoice(&plan, 4)
                .review
                .contains(&ReviewReason::DuplicateNumber)
        );
        assert!(
            invoice(&plan, 5)
                .review
                .contains(&ReviewReason::DuplicateNumber)
        );
        assert!(
            !invoice(&plan, 1)
                .review
                .contains(&ReviewReason::DuplicateNumber)
        );
    }
}
