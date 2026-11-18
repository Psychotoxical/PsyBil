//! The invoice PDF: preview pictures, file name and saving.

use tauri::State;
use uuid::Uuid;

use crate::error::CmdResult;
use crate::state::AppState;

/// One base64 PNG per page. Rendering takes a while, so it runs off the main thread.
#[tauri::command(async, rename_all = "snake_case")]
pub fn preview_png(state: State<'_, AppState>, id: Uuid, width_px: u32) -> CmdResult<Vec<String>> {
    use base64::Engine;
    let pages = state.read(|c| psybil_core::pdf::preview::preview_png(c, id, width_px))?;
    Ok(pages
        .iter()
        .map(|p| base64::engine::general_purpose::STANDARD.encode(p))
        .collect())
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn pdf_file_name(state: State<'_, AppState>, id: Uuid) -> CmdResult<String> {
    state.read(|c| psybil_core::pdf::export::pdf_file_name_for(c, id))
}

pub fn export_pdf_inner(state: &AppState, id: Uuid, path: &str) -> CmdResult<String> {
    let written =
        state.read(|c| psybil_core::pdf::export_pdf(c, id, std::path::Path::new(path)))?;
    state.note_exported(&written);
    Ok(written.to_string_lossy().into_owned())
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn export_pdf(state: State<'_, AppState>, id: Uuid, path: String) -> CmdResult<String> {
    export_pdf_inner(&state, id, &path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::demo_state;
    use psybil_core::invoice::InvoiceState;
    use psybil_core::listing::{InvoiceFilter, list_invoices};

    #[test]
    fn export_pdf_writes_archived_bytes() {
        let (state, dirs) = demo_state();
        let issued = state
            .read(|c| {
                list_invoices(
                    c,
                    &InvoiceFilter::default(),
                    chrono::Local::now().date_naive(),
                )
            })
            .unwrap()
            .into_iter()
            .find(|r| r.state == InvoiceState::Issued)
            .unwrap();
        let target = dirs.0.path().join("x.pdf");
        let written = export_pdf_inner(&state, issued.id, target.to_str().unwrap()).unwrap();
        assert_eq!(written, target.to_string_lossy());
        let archived = state
            .read(|c| psybil_core::pdf::invoice_pdf(c, issued.id))
            .unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), archived);
    }
}
