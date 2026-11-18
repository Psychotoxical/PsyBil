//! Taking over data from VibeBill: read an old database, let the window
//! correct the plan, then apply it after a backup.

use std::path::{Path, PathBuf};

use psybil_core::backup::{BEFORE_IMPORT_PREFIX, backup_file_name, backup_to};
use psybil_core::dates::SystemClock;
use psybil_core::legacy::{
    self, ImportPlan, ImportReport, LegacyData, LegacyInvoiceRow, invoice_rows, plan_import,
};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::error::{CmdError, CmdResult};
use crate::state::{AppState, local_now, lock};

/// An old database that was read and waits for "Take over".
pub struct PendingImport {
    pub token: Uuid,
    pub data: LegacyData,
}

#[derive(Debug, Clone, Serialize)]
pub struct LegacySellerRow {
    pub id: i64,
    pub name: String,
}

/// An old customer or item that looks like one PsyBil has already.
#[derive(Debug, Clone, Serialize)]
pub struct LegacyMatch {
    pub legacy_id: i64,
    pub legacy_name: String,
    pub existing_id: Uuid,
    pub existing_name: String,
}

/// What the assistant shows: the old senders, the matches and every old
/// invoice, with the core's proposed plan.
#[derive(Debug, Clone, Serialize)]
pub struct LegacyPreview {
    pub token: Uuid,
    pub path: String,
    pub sellers: Vec<LegacySellerRow>,
    pub invoices: Vec<LegacyInvoiceRow>,
    pub customer_matches: Vec<LegacyMatch>,
    pub product_matches: Vec<LegacyMatch>,
    pub warnings: Vec<String>,
    pub plan: ImportPlan,
}

/// A private folder for the copy of the old database; gone when dropped.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn io(e: impl std::fmt::Display) -> CmdError {
    CmdError::new("io", e.to_string())
}

pub fn find_legacy_databases_inner() -> Vec<String> {
    legacy::default_config_dir()
        .map(|dir| legacy::find_legacy_databases(&dir))
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

pub fn read_legacy_inner(state: &AppState, path: &str) -> CmdResult<LegacyPreview> {
    let scratch = Scratch(std::env::temp_dir().join(format!("psybil-legacy-{}", Uuid::new_v4())));
    std::fs::create_dir_all(&scratch.0).map_err(io)?;
    // `read_legacy` reads a copy it makes in the scratch folder.
    let data = legacy::read_legacy(Path::new(path), &scratch.0)?;
    drop(scratch);
    let plan = state.read(|c| plan_import(c, &data))?;
    let matches = |pairs: &[(i64, Option<Uuid>)],
                   legacy_name: &dyn Fn(i64) -> String,
                   name: &dyn Fn(Uuid) -> String| {
        pairs
            .iter()
            .filter_map(|(legacy_id, existing)| {
                existing.map(|existing_id| LegacyMatch {
                    legacy_id: *legacy_id,
                    legacy_name: legacy_name(*legacy_id),
                    existing_id,
                    existing_name: name(existing_id),
                })
            })
            .collect::<Vec<_>>()
    };
    let customers = state.read(|c| psybil_core::masterdata::list_customers(c, true))?;
    // Items of every brand, by id, for naming the proposed matches.
    let products: Vec<(String, String)> = state.read(|c| {
        let mut stmt = c.prepare("SELECT id, name FROM products")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<psybil_core::rusqlite::Result<_>>()?)
    })?;
    let customer_matches = matches(
        &plan.customer_matches,
        &|id| {
            data.customers
                .iter()
                .find(|c| c.id == id)
                .map(|c| c.name.clone())
                .unwrap_or_default()
        },
        &|id| {
            customers
                .iter()
                .find(|c| c.id == id)
                .map(|c| c.name.clone())
                .unwrap_or_default()
        },
    );
    let product_matches = matches(
        &plan.product_matches,
        &|id| {
            data.products
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.name.clone())
                .unwrap_or_default()
        },
        &|id| {
            products
                .iter()
                .find(|(pid, _)| *pid == id.to_string())
                .map(|(_, name)| name.clone())
                .unwrap_or_default()
        },
    );
    let preview = LegacyPreview {
        token: Uuid::new_v4(),
        path: path.to_string(),
        sellers: data
            .sellers
            .iter()
            .map(|s| LegacySellerRow {
                id: s.id,
                name: s.name.clone(),
            })
            .collect(),
        invoices: invoice_rows(&data),
        customer_matches,
        product_matches,
        warnings: data.warnings.clone(),
        plan,
    };
    *lock(&state.pending_import) = Some(PendingImport {
        token: preview.token,
        data,
    });
    Ok(preview)
}

pub fn apply_import_inner(
    state: &AppState,
    token: Uuid,
    plan: &ImportPlan,
) -> CmdResult<ImportReport> {
    let mut pending = lock(&state.pending_import);
    let Some(import) = pending.as_ref().filter(|p| p.token == token) else {
        return Err(CmdError::new(
            "not_found",
            "the old database must be read again",
        ));
    };
    let dir = state.backups_dir();
    std::fs::create_dir_all(&dir).map_err(io)?;
    let backup = dir.join(backup_file_name(BEFORE_IMPORT_PREFIX, local_now()));
    state.with_workspace(|ws| backup_to(ws.db(), &backup))?;
    let report = state.write(|tx| legacy::apply_import(tx, &SystemClock, &import.data, plan))?;
    *pending = None;
    Ok(report)
}

/// Original PDFs larger than this are refused.
const MAX_PDF: u64 = 50 * 1024 * 1024;

pub fn attach_original_pdf_inner(state: &AppState, invoice_id: Uuid, path: &str) -> CmdResult<()> {
    let path = Path::new(path);
    let size = std::fs::metadata(path).map_err(io)?.len();
    if size > MAX_PDF {
        return Err(CmdError::new("validation", "the PDF is larger than 50 MB"));
    }
    let bytes = std::fs::read(path).map_err(io)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "original.pdf".into());
    state.write(|tx| {
        psybil_core::attachments::attach_original_pdf(tx, &SystemClock, invoice_id, &name, &bytes)
    })?;
    Ok(())
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn find_legacy_databases() -> Vec<String> {
    find_legacy_databases_inner()
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn read_legacy(state: State<'_, AppState>, path: String) -> CmdResult<LegacyPreview> {
    read_legacy_inner(&state, &path)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn apply_import(
    state: State<'_, AppState>,
    token: Uuid,
    plan: ImportPlan,
) -> CmdResult<ImportReport> {
    apply_import_inner(&state, token, &plan)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn cancel_import(state: State<'_, AppState>) {
    *lock(&state.pending_import) = None;
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn attach_original_pdf(
    state: State<'_, AppState>,
    invoice_id: Uuid,
    path: String,
) -> CmdResult<()> {
    attach_original_pdf_inner(&state, invoice_id, &path)
}

/// The file name of the attached original, if there is one.
#[tauri::command(async, rename_all = "snake_case")]
pub fn original_pdf_name(
    state: State<'_, AppState>,
    invoice_id: Uuid,
) -> CmdResult<Option<String>> {
    Ok(state
        .read(|c| psybil_core::attachments::original_pdf(c, invoice_id))?
        .map(|(name, _)| name))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn export_original_pdf(
    state: State<'_, AppState>,
    invoice_id: Uuid,
    path: String,
) -> CmdResult<String> {
    let (_, bytes) = state
        .read(|c| psybil_core::attachments::original_pdf(c, invoice_id))?
        .ok_or_else(|| CmdError::new("not_found", "no original PDF attached"))?;
    let written = psybil_core::pdf::export::write_file(Path::new(&path), &bytes)?;
    state.note_exported(&written);
    Ok(written.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::state_with;
    use psybil_core::backup::{BackupKind, list_backups};
    use psybil_core::issue::sha256_hex;
    use psybil_core::legacy::LegacyClass;
    use psybil_core::legacy::testdata::full_db;
    use psybil_core::settings::DataPlace;

    fn old_db() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vibebill.db");
        full_db(&path);
        (dir, path)
    }

    #[test]
    fn read_legacy_keeps_original_untouched() {
        let (state, _dirs) = state_with(DataPlace::Local);
        let (_old, path) = old_db();
        let before = sha256_hex(&std::fs::read(&path).unwrap());
        let preview = read_legacy_inner(&state, &path.to_string_lossy()).unwrap();
        assert_eq!(sha256_hex(&std::fs::read(&path).unwrap()), before);
        assert_eq!(preview.sellers.len(), 2);
        assert_eq!(preview.invoices.len(), preview.plan.invoices.len());
        assert!(lock(&state.pending_import).is_some());
    }

    #[test]
    fn apply_import_uses_edited_plan() {
        let (state, _dirs) = state_with(DataPlace::Local);
        let (_old, path) = old_db();
        let preview = read_legacy_inner(&state, &path.to_string_lossy()).unwrap();
        let mut plan = preview.plan.clone();
        for i in &mut plan.invoices {
            i.class = LegacyClass::NeverSent;
        }
        let report = apply_import_inner(&state, preview.token, &plan).unwrap();
        assert_eq!(report.issued, 0);
        assert!(report.drafts > 1, "{report:?}");
        // The plan is used once.
        assert_eq!(
            apply_import_inner(&state, preview.token, &plan)
                .unwrap_err()
                .code,
            "not_found"
        );
    }

    #[test]
    fn import_makes_backup_first() {
        let (state, _dirs) = state_with(DataPlace::Local);
        let (_old, path) = old_db();
        let preview = read_legacy_inner(&state, &path.to_string_lossy()).unwrap();
        apply_import_inner(&state, preview.token, &preview.plan).unwrap();
        let kinds: Vec<BackupKind> = list_backups(&state.backups_dir())
            .unwrap()
            .into_iter()
            .map(|b| b.kind)
            .collect();
        assert!(kinds.contains(&BackupKind::BeforeImport));
    }

    #[test]
    fn second_import_reports_already_present() {
        let (state, _dirs) = state_with(DataPlace::Local);
        let (_old, path) = old_db();
        let first = read_legacy_inner(&state, &path.to_string_lossy()).unwrap();
        apply_import_inner(&state, first.token, &first.plan).unwrap();
        let second = read_legacy_inner(&state, &path.to_string_lossy()).unwrap();
        let report = apply_import_inner(&state, second.token, &second.plan).unwrap();
        assert!(report.already_present > 0);
        assert_eq!(report.businesses, 0);
    }
}
