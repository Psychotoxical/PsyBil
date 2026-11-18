//! Invoices: drafts, issuing, cancellation and correction.

use psybil_core::calc::{PriceMode, Totals};
use psybil_core::dates::{Clock, SystemClock};
use psybil_core::invoice::{self, DraftInput, Invoice, LineDraft};
use psybil_core::issue::{IssueOptions, Issued};
use psybil_core::listing::{InvoiceFilter, InvoiceRow};
use psybil_core::payments::InvoiceStatus;
use psybil_core::validate::Issue;
use tauri::State;
use uuid::Uuid;

use crate::error::CmdResult;
use crate::state::AppState;

/// A few queries per row; off the main thread so typing in the search stays smooth.
#[tauri::command(async, rename_all = "snake_case")]
pub fn list_invoices(
    state: State<'_, AppState>,
    filter: InvoiceFilter,
) -> CmdResult<Vec<InvoiceRow>> {
    state.read(|c| psybil_core::listing::list_invoices(c, &filter, SystemClock.today()))
}

/// A catalogue product as a new line, its price in the draft's price mode.
pub fn product_line_inner(
    state: &AppState,
    product_id: Uuid,
    price_mode: PriceMode,
) -> CmdResult<LineDraft> {
    let product = state.read(|c| psybil_core::masterdata::get_product(c, product_id))?;
    Ok(invoice::product_line(&product, price_mode))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn product_line(
    state: State<'_, AppState>,
    product_id: Uuid,
    price_mode: PriceMode,
) -> CmdResult<LineDraft> {
    product_line_inner(&state, product_id, price_mode)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn load_invoice(state: State<'_, AppState>, id: Uuid) -> CmdResult<Invoice> {
    state.read(|c| invoice::load_invoice(c, id))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn create_draft(state: State<'_, AppState>, input: DraftInput) -> CmdResult<Uuid> {
    state.write(|tx| invoice::create_draft(tx, &SystemClock, input))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn save_draft(state: State<'_, AppState>, id: Uuid, input: DraftInput) -> CmdResult<()> {
    state.write(|tx| invoice::save_draft(tx, &SystemClock, id, input))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn delete_draft(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    state.write(|tx| invoice::delete_draft(tx, &SystemClock, id))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn draft_totals(state: State<'_, AppState>, id: Uuid) -> CmdResult<Totals> {
    state.read(|c| invoice::draft_totals(c, id))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn check_issue(state: State<'_, AppState>, id: Uuid) -> CmdResult<Vec<Issue>> {
    state.read(|c| psybil_core::validate::check_issue(c, id))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn peek_number(state: State<'_, AppState>, id: Uuid) -> CmdResult<String> {
    state.read(|c| psybil_core::numbering::peek_number(c, id))
}

/// Issuing renders and archives the PDF; it runs off the main thread.
#[tauri::command(async, rename_all = "snake_case")]
pub fn issue_invoice(
    state: State<'_, AppState>,
    id: Uuid,
    payment_received: bool,
) -> CmdResult<Issued> {
    state.write(|tx| {
        psybil_core::issue::issue_invoice(tx, &SystemClock, id, IssueOptions { payment_received })
    })
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn cancel_invoice(state: State<'_, AppState>, id: Uuid) -> CmdResult<Uuid> {
    state.write(|tx| psybil_core::storno::cancel_invoice(tx, &SystemClock, id))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn correct_invoice(state: State<'_, AppState>, id: Uuid) -> CmdResult<(Uuid, Uuid)> {
    state.write(|tx| psybil_core::storno::correct_invoice(tx, &SystemClock, id))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn duplicate_as_draft(state: State<'_, AppState>, id: Uuid) -> CmdResult<Uuid> {
    state.write(|tx| invoice::duplicate_as_draft(tx, &SystemClock, id))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn invoice_status(state: State<'_, AppState>, id: Uuid) -> CmdResult<InvoiceStatus> {
    state.read(|c| psybil_core::payments::invoice_status(c, id, SystemClock.today()))
}

#[cfg(test)]
mod tests {
    use psybil_core::calc::LineKind;
    use psybil_core::masterdata;

    use super::*;
    use crate::state::tests::demo_state;

    #[test]
    fn product_line_comes_from_the_catalogue() {
        let (state, _dirs) = demo_state();
        let product = state
            .read(|c| {
                let business = masterdata::list_businesses(c, false)?.remove(0);
                let brand = masterdata::list_brands(c, business.id)?.remove(0);
                Ok(masterdata::list_products(c, brand.id, false)?.remove(0))
            })
            .unwrap();
        let line = product_line_inner(&state, product.id, product.price_mode).unwrap();
        assert_eq!(line.kind, LineKind::Item);
        assert_eq!(line.product_id, Some(product.id));
        assert_eq!(line.unit_price, product.price);
        let other = match product.price_mode {
            PriceMode::Net => PriceMode::Gross,
            PriceMode::Gross => PriceMode::Net,
        };
        assert!(product_line_inner(&state, product.id, other).is_ok());
        let missing = product_line_inner(&state, Uuid::new_v4(), other).unwrap_err();
        assert_eq!(missing.code, "not_found");
    }
}
