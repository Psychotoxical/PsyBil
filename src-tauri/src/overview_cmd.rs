//! The overview page and the year summary.

use psybil_core::dates::SystemClock;
use psybil_core::overview::{self, OutsideRevenue, Overview};
use psybil_core::year_summary::{self, YearSummary};
use tauri::State;
use uuid::Uuid;

use crate::error::CmdResult;
use crate::payment_cmd::parse_amount;
use crate::state::AppState;

fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

pub fn set_outside_revenue_inner(
    state: &AppState,
    business_id: Uuid,
    year: i32,
    amount: &str,
    note: &str,
) -> CmdResult<()> {
    let cents = parse_amount(amount)?;
    state
        .write(|tx| overview::set_outside_revenue(tx, &SystemClock, business_id, year, cents, note))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn overview(state: State<'_, AppState>, business_id: Option<Uuid>) -> CmdResult<Overview> {
    state.read(|c| overview::overview(c, business_id, today()))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn set_outside_revenue(
    state: State<'_, AppState>,
    business_id: Uuid,
    year: i32,
    amount: String,
    note: String,
) -> CmdResult<()> {
    set_outside_revenue_inner(&state, business_id, year, &amount, &note)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn outside_revenue(
    state: State<'_, AppState>,
    business_id: Uuid,
    year: i32,
) -> CmdResult<Option<OutsideRevenue>> {
    state.read(|c| overview::outside_revenue(c, business_id, year))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn year_summary(
    state: State<'_, AppState>,
    business_id: Uuid,
    year: i32,
) -> CmdResult<YearSummary> {
    state.read(|c| year_summary::year_summary(c, business_id, year, today()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::demo_state;
    use psybil_core::money::Cents;

    #[test]
    fn outside_revenue_takes_decimal_text() {
        let (state, _dirs) = demo_state();
        let business = state
            .read(|c| psybil_core::masterdata::list_businesses(c, false))
            .unwrap()[0]
            .id;
        set_outside_revenue_inner(&state, business, 2026, "1234.50", "Merch").unwrap();
        let got = state
            .read(|c| overview::outside_revenue(c, business, 2026))
            .unwrap()
            .unwrap();
        assert_eq!(got.amount, Cents(123_450));
        assert_eq!(
            set_outside_revenue_inner(&state, business, 2026, "1.234", "")
                .unwrap_err()
                .code,
            "invalid_amount"
        );
    }
}
