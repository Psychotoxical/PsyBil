//! Year report and CSV files, and showing written files in the file manager.

use std::path::{Path, PathBuf};

use psybil_core::csv_export;
use psybil_core::listing::InvoiceFilter;
use psybil_core::pdf::Lang;
use psybil_core::pdf::export::write_file;
use psybil_core::pdf::year_report;
use psybil_core::year_summary::year_summary;
use tauri::State;
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;

use crate::error::{CmdError, CmdResult};
use crate::state::{AppState, lock};

fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

/// The language of the window, which also decides the CSV format.
fn lang(state: &AppState) -> Lang {
    Lang::from_code(&lock(&state.settings).language)
}

fn write(state: &AppState, path: &str, bytes: &[u8]) -> CmdResult<String> {
    let written = write_file(Path::new(path), bytes)?;
    state.note_exported(&written);
    Ok(written.to_string_lossy().into_owned())
}

pub fn export_year_report_inner(
    state: &AppState,
    business_id: Uuid,
    year: i32,
    path: &str,
    format: &str,
) -> CmdResult<String> {
    let summary = state.read(|c| year_summary(c, business_id, year, today()))?;
    let bytes = match format {
        "pdf" => year_report::year_report_pdf(&summary, lang(state))?,
        "csv" => csv_export::year_summary_csv(&summary, lang(state)).into_bytes(),
        _ => {
            return Err(CmdError::new(
                "invalid_format",
                format!("unknown format {format:?}"),
            ));
        }
    };
    write(state, path, &bytes)
}

pub fn export_invoices_csv_inner(
    state: &AppState,
    filter: &InvoiceFilter,
    path: &str,
) -> CmdResult<String> {
    let csv = state.read(|c| csv_export::invoices_csv(c, filter, today(), lang(state)))?;
    write(state, path, csv.as_bytes())
}

pub fn export_file_name_inner(
    state: &AppState,
    kind: &str,
    business_id: Option<Uuid>,
    year: Option<i32>,
) -> CmdResult<String> {
    let lang = lang(state);
    match (kind, business_id, year) {
        ("invoices_csv", _, _) => Ok(csv_export::invoices_csv_file_name(today(), lang)),
        ("year_pdf" | "year_csv", Some(business_id), Some(year)) => {
            let summary = state.read(|c| year_summary(c, business_id, year, today()))?;
            let ext = if kind == "year_pdf" { "pdf" } else { "csv" };
            Ok(year_report::year_report_file_name(&summary, lang, ext))
        }
        _ => Err(CmdError::new(
            "invalid_format",
            format!("unknown file kind {kind:?}"),
        )),
    }
}

/// `path` if PsyBil wrote it in this session; nothing else is shown.
pub fn check_exported(state: &AppState, path: &str) -> CmdResult<PathBuf> {
    let path = PathBuf::from(path);
    if lock(&state.exported).contains(&path) {
        Ok(path)
    } else {
        Err(CmdError::new(
            "not_found",
            "this file was not written by PsyBil",
        ))
    }
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn export_year_report(
    state: State<'_, AppState>,
    business_id: Uuid,
    year: i32,
    path: String,
    format: String,
) -> CmdResult<String> {
    export_year_report_inner(&state, business_id, year, &path, &format)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn export_invoices_csv(
    state: State<'_, AppState>,
    filter: InvoiceFilter,
    path: String,
) -> CmdResult<String> {
    export_invoices_csv_inner(&state, &filter, &path)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn export_file_name(
    state: State<'_, AppState>,
    kind: String,
    business_id: Option<Uuid>,
    year: Option<i32>,
) -> CmdResult<String> {
    export_file_name_inner(&state, &kind, business_id, year)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn reveal_exported(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<()> {
    let path = check_exported(&state, &path)?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| CmdError::new("io", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::demo_state;

    #[test]
    fn exports_write_files_and_only_those_can_be_revealed() {
        let (state, dirs) = demo_state();
        let business = state
            .read(|c| psybil_core::masterdata::list_businesses(c, false))
            .unwrap()[0]
            .id;
        let year = today().format("%Y").to_string().parse().unwrap();
        let pdf = dirs.0.path().join("jahr.pdf");
        export_year_report_inner(&state, business, year, &pdf.to_string_lossy(), "pdf").unwrap();
        assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF-"));
        let csv = dirs.0.path().join("liste.csv");
        export_invoices_csv_inner(&state, &InvoiceFilter::default(), &csv.to_string_lossy())
            .unwrap();
        let text = std::fs::read_to_string(&csv).unwrap();
        assert!(text.starts_with("\u{feff}Nummer;"));
        assert!(check_exported(&state, &pdf.to_string_lossy()).is_ok());
        assert_eq!(
            check_exported(&state, "C:/Windows/notepad.exe")
                .unwrap_err()
                .code,
            "not_found"
        );
        assert_eq!(
            export_year_report_inner(&state, business, year, &pdf.to_string_lossy(), "doc")
                .unwrap_err()
                .code,
            "invalid_format"
        );
    }

    #[test]
    fn file_names_follow_the_language() {
        let (state, _dirs) = demo_state();
        let name = export_file_name_inner(&state, "invoices_csv", None, None).unwrap();
        assert!(name.starts_with("Rechnungen-"), "{name}");
        lock(&state.settings).language = "en".into();
        let name = export_file_name_inner(&state, "invoices_csv", None, None).unwrap();
        assert!(name.starts_with("Invoices-"), "{name}");
    }
}
