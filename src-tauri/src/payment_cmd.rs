//! Payments.

use chrono::NaiveDate;
use psybil_core::dates::SystemClock;
use psybil_core::money::Cents;
use psybil_core::payments::{self, Payment, PaymentMethod};
use tauri::State;
use uuid::Uuid;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// An amount from the window ("12.50", "-5") in cents; more than two
/// decimals are refused, nothing is rounded.
pub fn parse_amount(text: &str) -> CmdResult<Cents> {
    use psybil_core::rust_decimal::Decimal;
    use psybil_core::rust_decimal::prelude::ToPrimitive;
    let invalid = || {
        CmdError::new(
            "invalid_amount",
            format!("not an amount in cents: {text:?}"),
        )
    };
    let value: Decimal = text.trim().parse().map_err(|_| invalid())?;
    let cents = value * Decimal::ONE_HUNDRED;
    if !cents.fract().is_zero() {
        return Err(invalid());
    }
    Ok(Cents(cents.to_i64().ok_or_else(invalid)?))
}

pub fn add_payment_inner(
    state: &AppState,
    invoice_id: Uuid,
    amount: &str,
    date: NaiveDate,
    method: PaymentMethod,
    note: &str,
) -> CmdResult<Uuid> {
    let cents = parse_amount(amount)?;
    state.write(|tx| payments::add_payment(tx, &SystemClock, invoice_id, cents, date, method, note))
}

#[tauri::command(rename_all = "snake_case")]
pub fn add_payment(
    state: State<'_, AppState>,
    invoice_id: Uuid,
    amount: String,
    date: NaiveDate,
    method: PaymentMethod,
    note: String,
) -> CmdResult<Uuid> {
    add_payment_inner(&state, invoice_id, &amount, date, method, &note)
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_payments(state: State<'_, AppState>, invoice_id: Uuid) -> CmdResult<Vec<Payment>> {
    state.read(|c| payments::list_payments(c, invoice_id))
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_payment(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    state.write(|tx| payments::delete_payment(tx, &SystemClock, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::demo_state;
    use psybil_core::listing::{InvoiceFilter, list_invoices};
    use psybil_core::payments::StatusKind;

    #[test]
    fn add_payment_rejects_fractions_of_cents() {
        assert_eq!(parse_amount("12.34").unwrap(), Cents(1234));
        assert_eq!(parse_amount("-5").unwrap(), Cents(-500));
        assert_eq!(parse_amount("12.345").unwrap_err().code, "invalid_amount");
        assert_eq!(parse_amount("zwölf").unwrap_err().code, "invalid_amount");

        let (state, _dirs) = demo_state();
        let open = state
            .read(|c| {
                list_invoices(
                    c,
                    &InvoiceFilter::default(),
                    chrono::Local::now().date_naive(),
                )
            })
            .unwrap()
            .into_iter()
            .find(|r| r.status.kind == StatusKind::Open)
            .unwrap();
        let today = chrono::Local::now().date_naive();
        assert!(
            add_payment_inner(&state, open.id, "12.345", today, PaymentMethod::Cash, "").is_err()
        );
        add_payment_inner(&state, open.id, "12.34", today, PaymentMethod::Cash, "").unwrap();
        let paid: i64 = state
            .read(|c| payments::list_payments(c, open.id))
            .unwrap()
            .iter()
            .map(|p| p.amount.0)
            .sum();
        assert_eq!(paid, 1234);
    }
}
