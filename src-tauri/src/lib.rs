//! The PsyBil window: thin commands over `psybil-core`.

pub mod error;
mod invoice_cmd;
mod masterdata_cmd;
mod payment_cmd;
mod pdf_cmd;
mod settings_cmd;
pub mod state;

use tauri::Manager;

/// The frontend has drawn its first frame: show the window.
#[tauri::command]
fn frontend_ready(window: tauri::WebviewWindow) {
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn run() {
    let demo = std::env::args().any(|a| a == "--demo");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let data_dir = app.path().app_data_dir()?;
            let config_dir = app.path().app_config_dir()?;
            let state = state::AppState::open(&data_dir, &config_dir, demo)?;
            if demo {
                // Folders of earlier demo runs; the app may end without dropping its state.
                if let Some(current) = state.settings_path.parent() {
                    state::remove_stale_demo_folders(&std::env::temp_dir(), current);
                }
            }
            app.manage(state);
            if let Some(window) = app.get_webview_window("main") {
                if demo {
                    let _ = window.set_title("PsyBil (Demo)");
                }
                // Show the window even if the frontend never reports.
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(4));
                    let _ = window.show();
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            frontend_ready,
            settings_cmd::load_settings,
            settings_cmd::save_settings,
            masterdata_cmd::list_businesses,
            masterdata_cmd::create_business,
            masterdata_cmd::update_business,
            masterdata_cmd::archive_business,
            masterdata_cmd::delete_business,
            masterdata_cmd::list_brands,
            masterdata_cmd::create_brand,
            masterdata_cmd::update_brand,
            masterdata_cmd::set_brand_logo,
            masterdata_cmd::remove_brand_logo,
            masterdata_cmd::brand_logo,
            masterdata_cmd::archive_brand,
            masterdata_cmd::delete_brand,
            masterdata_cmd::list_customers,
            masterdata_cmd::create_customer,
            masterdata_cmd::update_customer,
            masterdata_cmd::archive_customer,
            masterdata_cmd::delete_customer,
            masterdata_cmd::list_products,
            masterdata_cmd::create_product,
            masterdata_cmd::update_product,
            masterdata_cmd::delete_product,
            invoice_cmd::list_invoices,
            invoice_cmd::load_invoice,
            invoice_cmd::create_draft,
            invoice_cmd::save_draft,
            invoice_cmd::delete_draft,
            invoice_cmd::draft_totals,
            invoice_cmd::product_line,
            invoice_cmd::check_issue,
            invoice_cmd::peek_number,
            invoice_cmd::issue_invoice,
            invoice_cmd::cancel_invoice,
            invoice_cmd::correct_invoice,
            invoice_cmd::duplicate_as_draft,
            invoice_cmd::invoice_status,
            payment_cmd::list_payments,
            payment_cmd::add_payment,
            payment_cmd::delete_payment,
            pdf_cmd::preview_png,
            pdf_cmd::pdf_file_name,
            pdf_cmd::export_pdf,
        ])
        .run(tauri::generate_context!())
        .expect("PsyBil failed to start");
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    fn read(file: &str) -> String {
        std::fs::read_to_string(format!("{}/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    /// Names in quotes or paths between `start` and the next `end`.
    fn names(text: &str, start: &str, end: &str) -> BTreeSet<String> {
        let block = text.split(start).nth(1).unwrap().split(end).next().unwrap();
        block
            .split(',')
            .map(|n| n.trim().trim_matches('"'))
            .filter(|n| !n.is_empty())
            .map(|n| n.rsplit("::").next().unwrap().to_string())
            .collect()
    }

    #[test]
    fn every_command_has_handler_and_permission() {
        let commands = names(&read("build.rs"), "const COMMANDS: &[&str] = &[", "];");
        let handlers = names(&read("src/lib.rs"), "generate_handler![", "]");
        let caps: serde_json::Value =
            serde_json::from_str(&read("capabilities/main.json")).unwrap();
        let granted: BTreeSet<String> = caps["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p.as_str()?.strip_prefix("allow-"))
            .map(|p| p.replace('-', "_"))
            .collect();
        assert!(commands.len() > 40);
        assert_eq!(commands, handlers, "build.rs vs generate_handler");
        assert_eq!(commands, granted, "build.rs vs capabilities/main.json");
    }
}
