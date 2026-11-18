//! Checks the mandatory contents of an invoice before it is issued
//! (§ 14 UStG, § 33 UStDV for small amounts, § 34a UStDV for small
//! businesses).

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Result;
use crate::dates::ServiceDate;
use crate::invoice::{InvoiceKind, draft_totals, load_invoice};
use crate::masterdata::{CustomerKind, get_business, get_customer};
use crate::money::Cents;
use crate::tax::{TaxCategory, TaxChoice};

/// Gross amount up to which the reduced rules for small amounts apply.
pub const SMALL_AMOUNT_LIMIT: Cents = Cents(25_000);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum Severity {
    /// Blocks issuing.
    Error,
    /// Shown, but does not block.
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum IssueKind {
    BusinessName,
    BusinessAddress,
    BusinessTaxId,
    NoLines,
    LineDescription { position: u32 },
    Customer,
    CustomerAddress,
    CustomerKindUnknown,
    ServiceDate,
    ExemptReason { position: u32 },
    TaxRate { position: u32 },
    SmallBusinessLine { position: u32 },
    CustomerVatId,
    BusinessVatId,
    NegativeTotal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Issue {
    pub kind: IssueKind,
    pub severity: Severity,
}

fn blank(text: &str) -> bool {
    text.trim().is_empty()
}

/// Lists everything that is missing or wrong for issuing the invoice.
/// Entries with `Severity::Error` block issuing.
pub fn check_issue(conn: &Connection, invoice_id: Uuid) -> Result<Vec<Issue>> {
    let invoice = load_invoice(conn, invoice_id)?;
    let business = get_business(conn, invoice.business_id)?;
    let customer = invoice
        .customer_id
        .map(|id| get_customer(conn, id))
        .transpose()?;
    let totals = draft_totals(conn, invoice_id)?;

    let mut issues = Vec::new();
    let mut error = |kind: IssueKind| {
        if !issues.iter().any(|i: &Issue| i.kind == kind) {
            issues.push(Issue {
                kind,
                severity: Severity::Error,
            });
        }
    };

    if blank(&business.name) {
        error(IssueKind::BusinessName);
    }
    if blank(&business.street) || blank(&business.zip) || blank(&business.city) {
        error(IssueKind::BusinessAddress);
    }
    if blank(&business.tax_number) && blank(&business.vat_id) {
        error(IssueKind::BusinessTaxId);
    }
    if invoice.lines.is_empty() {
        error(IssueKind::NoLines);
    }
    for (index, line) in invoice.lines.iter().enumerate() {
        if blank(&line.description) {
            error(IssueKind::LineDescription {
                position: index as u32 + 1,
            });
        }
    }

    // Reverse charge and intra-community supplies never get the small-amount
    // simplification (§ 33 Satz 3 UStDV).
    let needs_vat_ids = !business.small_business
        && invoice.lines.iter().any(|line| {
            matches!(
                line.tax,
                TaxChoice::Fixed(TaxCategory::ReverseCharge | TaxCategory::IntraCommunity)
            )
        });
    let full_contents =
        !business.small_business && (totals.gross > SMALL_AMOUNT_LIMIT || needs_vat_ids);
    let customer_required = business.small_business || full_contents;
    let customer_address_missing = |c: &crate::masterdata::Customer| {
        blank(&c.name) || blank(&c.street) || blank(&c.zip) || blank(&c.city)
    };
    if customer_required {
        match &customer {
            None => error(IssueKind::Customer),
            Some(c) if customer_address_missing(c) => error(IssueKind::CustomerAddress),
            Some(_) => {}
        }
    }
    if full_contents && invoice.service_date == ServiceDate::None {
        error(IssueKind::ServiceDate);
    }

    if !business.small_business {
        for (index, line) in invoice.lines.iter().enumerate() {
            let position = index as u32 + 1;
            match &line.tax {
                TaxChoice::Fixed(TaxCategory::Exempt { reason }) if blank(reason) => {
                    error(IssueKind::ExemptReason { position });
                }
                // A 0 % line needs a reason, so it must be entered as exempt.
                TaxChoice::Fixed(TaxCategory::Standard { rate }) if rate.is_zero() => {
                    error(IssueKind::ExemptReason { position });
                }
                TaxChoice::Fixed(TaxCategory::Standard { rate })
                    if !business.tax_rates.contains(rate) =>
                {
                    error(IssueKind::TaxRate { position });
                }
                TaxChoice::Fixed(TaxCategory::SmallBusiness) => {
                    error(IssueKind::SmallBusinessLine { position });
                }
                _ => {}
            }
        }
        if needs_vat_ids {
            match &customer {
                None => error(IssueKind::Customer),
                Some(c) if blank(&c.vat_id) => error(IssueKind::CustomerVatId),
                Some(_) => {}
            }
            if blank(&business.vat_id) {
                error(IssueKind::BusinessVatId);
            }
        }
    }

    if invoice.kind == InvoiceKind::Invoice && totals.gross < Cents::ZERO {
        error(IssueKind::NegativeTotal);
    }

    if customer
        .as_ref()
        .is_some_and(|c| c.kind == CustomerKind::Unknown)
    {
        issues.push(Issue {
            kind: IssueKind::CustomerKindUnknown,
            severity: Severity::Warning,
        });
    }
    Ok(issues)
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use crate::db::Db;
    use crate::invoice::{DraftInput, create_draft};
    use crate::masterdata::{NewBusiness, NewCustomer, create_business, create_customer};
    use crate::testutil::{clock, sample_business, sample_customer, sample_draft};
    use rust_decimal::dec;

    fn errors_for(
        business: NewBusiness,
        customer: NewCustomer,
        edit: impl FnOnce(&mut DraftInput),
    ) -> Vec<IssueKind> {
        let mut db = Db::open_in_memory().unwrap();
        let (_, brand_id) = db
            .write(|tx| create_business(tx, &clock(), business))
            .unwrap();
        let customer_id = db
            .write(|tx| create_customer(tx, &clock(), customer))
            .unwrap();
        let mut input = sample_draft(brand_id, Some(customer_id));
        edit(&mut input);
        let id = db.write(|tx| create_draft(tx, &clock(), input)).unwrap();
        check_issue(db.read(), id)
            .unwrap()
            .into_iter()
            .filter(|i| i.severity == Severity::Error)
            .map(|i| i.kind)
            .collect()
    }

    #[test]
    fn rate_not_in_business_list_is_error() {
        let errors = errors_for(sample_business(), sample_customer(), |input| {
            input.lines[0].tax = TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(16) });
        });
        assert_eq!(errors, vec![IssueKind::TaxRate { position: 1 }]);
    }

    #[test]
    fn standard_zero_needs_reason() {
        let business = NewBusiness {
            tax_rates: vec![dec!(19), dec!(7), dec!(0)],
            ..sample_business()
        };
        let errors = errors_for(business, sample_customer(), |input| {
            input.lines[0].tax = TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(0) });
        });
        assert_eq!(errors, vec![IssueKind::ExemptReason { position: 1 }]);
    }

    #[test]
    fn small_business_line_on_regular_business_is_error() {
        let errors = errors_for(sample_business(), sample_customer(), |input| {
            input.lines[0].tax = TaxChoice::Fixed(TaxCategory::SmallBusiness);
        });
        assert_eq!(errors, vec![IssueKind::SmallBusinessLine { position: 1 }]);
    }

    #[test]
    fn reverse_charge_small_amount_needs_full_contents() {
        let business = NewBusiness {
            vat_id: "DE123456789".into(),
            ..sample_business()
        };
        let customer = NewCustomer {
            vat_id: "ATU12345678".into(),
            street: String::new(),
            ..sample_customer()
        };
        let errors = errors_for(business, customer, |input| {
            input.lines[0].quantity = dec!(1);
            input.lines[0].unit_price = dec!(100);
            input.lines[0].tax = TaxChoice::Fixed(TaxCategory::ReverseCharge);
            input.service_date = ServiceDate::None;
        });
        assert_eq!(
            errors,
            vec![IssueKind::CustomerAddress, IssueKind::ServiceDate]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::invoice::{DraftInput, create_draft};
    use crate::masterdata::{
        CustomerKind, NewBusiness, NewCustomer, create_business, create_customer,
    };
    use crate::tax::{TaxCategory, TaxChoice};
    use crate::testutil::{clock, sample_business, sample_customer, sample_draft};
    use rust_decimal::dec;

    fn issues_for(
        business: NewBusiness,
        customer: Option<NewCustomer>,
        edit: impl FnOnce(&mut DraftInput),
    ) -> Vec<Issue> {
        let mut db = Db::open_in_memory().unwrap();
        let (_, brand_id) = db
            .write(|tx| create_business(tx, &clock(), business))
            .unwrap();
        let customer_id =
            customer.map(|c| db.write(|tx| create_customer(tx, &clock(), c)).unwrap());
        let mut input = sample_draft(brand_id, customer_id);
        edit(&mut input);
        let id = db.write(|tx| create_draft(tx, &clock(), input)).unwrap();
        check_issue(db.read(), id).unwrap()
    }

    fn errors(issues: &[Issue]) -> Vec<IssueKind> {
        issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .map(|i| i.kind.clone())
            .collect()
    }

    fn single_line(input: &mut DraftInput, price: rust_decimal::Decimal, tax: TaxChoice) {
        input.lines[0].quantity = dec!(1);
        input.lines[0].unit_price = price;
        input.lines[0].tax = tax;
    }

    #[test]
    fn complete_regular_invoice_has_no_errors() {
        let issues = issues_for(sample_business(), Some(sample_customer()), |_| {});
        assert_eq!(issues, vec![]);
    }

    #[test]
    fn missing_tax_id_is_error() {
        let business = NewBusiness {
            tax_number: String::new(),
            ..sample_business()
        };
        let issues = issues_for(business, Some(sample_customer()), |_| {});
        assert_eq!(errors(&issues), vec![IssueKind::BusinessTaxId]);
    }

    #[test]
    fn missing_business_address_and_line_text() {
        let business = NewBusiness {
            city: String::new(),
            ..sample_business()
        };
        let issues = issues_for(business, Some(sample_customer()), |input| {
            input.lines[0].description = "  ".into();
        });
        assert_eq!(
            errors(&issues),
            vec![
                IssueKind::BusinessAddress,
                IssueKind::LineDescription { position: 1 }
            ]
        );
    }

    #[test]
    fn no_lines_is_error() {
        let issues = issues_for(sample_business(), Some(sample_customer()), |input| {
            input.lines.clear();
        });
        assert_eq!(errors(&issues), vec![IssueKind::NoLines]);
    }

    #[test]
    fn small_amount_needs_no_customer() {
        let issues = issues_for(sample_business(), None, |input| {
            single_line(
                input,
                dec!(210.08),
                TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(19) }),
            );
            input.service_date = ServiceDate::None;
        });
        assert_eq!(errors(&issues), vec![]);
    }

    #[test]
    fn over_250_needs_customer_and_service_date() {
        let issues = issues_for(sample_business(), None, |input| {
            single_line(
                input,
                dec!(210.09),
                TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(19) }),
            );
            input.service_date = ServiceDate::None;
        });
        assert_eq!(
            errors(&issues),
            vec![IssueKind::Customer, IssueKind::ServiceDate]
        );
    }

    #[test]
    fn small_business_needs_customer_address() {
        let business = NewBusiness {
            small_business: true,
            ..sample_business()
        };
        let customer = NewCustomer {
            street: String::new(),
            ..sample_customer()
        };
        let issues = issues_for(business, Some(customer), |input| {
            input.service_date = ServiceDate::None;
        });
        assert_eq!(errors(&issues), vec![IssueKind::CustomerAddress]);
    }

    #[test]
    fn small_business_small_amount_still_needs_customer() {
        let business = NewBusiness {
            small_business: true,
            ..sample_business()
        };
        let issues = issues_for(business, None, |_| {});
        assert_eq!(errors(&issues), vec![IssueKind::Customer]);
    }

    #[test]
    fn exempt_without_reason() {
        let issues = issues_for(sample_business(), Some(sample_customer()), |input| {
            input.lines[0].tax = TaxChoice::Fixed(TaxCategory::Exempt { reason: " ".into() });
        });
        assert_eq!(
            errors(&issues),
            vec![IssueKind::ExemptReason { position: 1 }]
        );
    }

    #[test]
    fn reverse_charge_needs_vat_ids() {
        let issues = issues_for(sample_business(), Some(sample_customer()), |input| {
            input.lines[0].tax = TaxChoice::Fixed(TaxCategory::ReverseCharge);
        });
        assert_eq!(
            errors(&issues),
            vec![IssueKind::CustomerVatId, IssueKind::BusinessVatId]
        );
    }

    #[test]
    fn unknown_customer_kind_is_warning_only() {
        let customer = NewCustomer {
            kind: CustomerKind::Unknown,
            ..sample_customer()
        };
        let issues = issues_for(sample_business(), Some(customer), |_| {});
        assert_eq!(errors(&issues), vec![]);
        assert_eq!(
            issues,
            vec![Issue {
                kind: IssueKind::CustomerKindUnknown,
                severity: Severity::Warning
            }]
        );
    }

    #[test]
    fn negative_invoice_total_is_error() {
        let issues = issues_for(sample_business(), Some(sample_customer()), |input| {
            input.lines[0].quantity = dec!(-1);
        });
        assert_eq!(errors(&issues), vec![IssueKind::NegativeTotal]);
    }
}
