//! The PsyBil window: thin commands over `psybil-core`.

pub mod background;
mod backup_cmd;
pub mod error;
mod export_cmd;
mod import_cmd;
mod invoice_cmd;
mod masterdata_cmd;
mod overview_cmd;
mod payment_cmd;
mod pdf_cmd;
mod settings_cmd;
pub mod state;
mod workspace_cmd;

use psybil_core::dates::SystemClock;
use tauri::{Emitter, Manager};

/// Runs the background beat every second and passes its news to the window.
fn spawn_background(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut ticker = background::Ticker::new(std::time::Instant::now());
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let state = app.state::<state::AppState>();
            let now = std::time::Instant::now();
            for event in background::tick(&state, &mut ticker, now, &SystemClock) {
                let _ = match event {
                    background::Event::Status => {
                        app.emit("workspace-status", workspace_cmd::status(&state))
                    }
                    background::Event::Merged(report) => app.emit("merge-report", report),
                };
            }
        }
    });
}

/// The frontend has drawn its first frame: show the window.
#[tauri::command(async)]
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
        // Used from Rust only, to show files PsyBil wrote; the window gets
        // no opener permission.
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            // `PSYBIL_HOME` keeps data and settings in one chosen folder, for
            // trying PsyBil out without touching this computer's real data.
            let (data_dir, config_dir) = match std::env::var_os("PSYBIL_HOME") {
                Some(home) => {
                    let home = std::path::PathBuf::from(home);
                    (home.join("data"), home.join("config"))
                }
                None => (app.path().app_data_dir()?, app.path().app_config_dir()?),
            };
            let state = state::AppState::open(&data_dir, &config_dir, demo)?;
            if demo {
                // Folders of earlier demo runs; the app may end without dropping its state.
                if let Some(current) = state.settings_path.parent() {
                    state::remove_stale_demo_folders(&std::env::temp_dir(), current);
                }
            }
            app.manage(state);
            spawn_background(app.handle().clone());
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
            overview_cmd::overview,
            overview_cmd::set_outside_revenue,
            overview_cmd::outside_revenue,
            overview_cmd::year_summary,
            export_cmd::export_year_report,
            export_cmd::export_invoices_csv,
            export_cmd::export_file_name,
            export_cmd::reveal_exported,
            backup_cmd::backup_now,
            backup_cmd::list_backups,
            backup_cmd::inspect_backup,
            backup_cmd::restore_backup,
            backup_cmd::reveal_backups_folder,
            workspace_cmd::workspace_status,
            workspace_cmd::reload_workspace,
            workspace_cmd::take_over_lock,
            workspace_cmd::force_take_over_lock,
            workspace_cmd::merge_conflict_copies,
            workspace_cmd::keep_other_version,
            workspace_cmd::quarantine_entries,
            workspace_cmd::choose_data_place,
            workspace_cmd::finish_setup,
            import_cmd::find_legacy_databases,
            import_cmd::read_legacy,
            import_cmd::apply_import,
            import_cmd::cancel_import,
            import_cmd::attach_original_pdf,
            import_cmd::original_pdf_name,
            import_cmd::export_original_pdf,
        ])
        .build(tauri::generate_context!())
        .expect("PsyBil failed to start")
        .run(|app, event| {
            // Write back and release the lock of a shared folder. When that
            // fails, the working copy keeps the changes for the next start.
            if let tauri::RunEvent::Exit = event
                && let Some(state) = app.try_state::<state::AppState>()
                && let Err(e) = state.close_workspace()
            {
                eprintln!("PsyBil: writing back at exit failed: {}", e.message);
            }
        });
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

    /// Commands that are not `async` run on the main thread; while the
    /// background writes back, they would freeze the window.
    #[test]
    fn every_command_runs_off_the_main_thread() {
        let dir = format!("{}/src", env!("CARGO_MANIFEST_DIR"));
        let mut blocking = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            for line in text.lines() {
                let line = line.trim();
                if line.starts_with("#[tauri::command") && !line.contains("async") {
                    blocking.push(format!("{}: {line}", path.display()));
                }
            }
        }
        assert!(blocking.is_empty(), "{blocking:#?}");
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

    /// A strict CSP without outside sources: the window loads nothing from
    /// the internet.
    #[test]
    fn csp_allows_no_outside_sources() {
        let conf: serde_json::Value = serde_json::from_str(&read("tauri.conf.json")).unwrap();
        let csp = conf["app"]["security"]["csp"].as_object().unwrap();
        assert!(csp.len() > 5);
        for (directive, sources) in csp {
            for source in sources.as_str().unwrap().split_whitespace() {
                let allowed = match source {
                    "'self'" | "'none'" | "data:" | "blob:" | "ipc:" | "http://ipc.localhost" => {
                        true
                    }
                    "'unsafe-inline'" => directive == "style-src",
                    _ => false,
                };
                assert!(allowed, "{directive}: {source}");
            }
        }
    }

    fn be_u32(bytes: &[u8], at: usize) -> u32 {
        u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    /// The icon set the bundles take: every PNG has the size its name says,
    /// the Windows icon carries the sizes Explorer and the taskbar pick
    /// from, and the macOS icon goes up to 1024 px.
    #[test]
    fn bundle_icons_exist_with_their_sizes() {
        let conf: serde_json::Value = serde_json::from_str(&read("tauri.conf.json")).unwrap();
        let icons = conf["bundle"]["icon"].as_array().unwrap();
        assert_eq!(icons.len(), 5);
        for icon in icons {
            let path = icon.as_str().unwrap();
            let bytes = std::fs::read(format!("{}/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap();
            let name = path.rsplit('/').next().unwrap();
            if let Some(stem) = name.strip_suffix(".png") {
                let (size, scale) = match stem.split_once('@') {
                    Some((size, "2x")) => (size, 2),
                    _ => (stem, 1),
                };
                let (w, h) = size.split_once('x').unwrap();
                let expected = (
                    w.parse::<u32>().unwrap() * scale,
                    h.parse::<u32>().unwrap() * scale,
                );
                assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "{path}");
                assert_eq!((be_u32(&bytes, 16), be_u32(&bytes, 20)), expected, "{path}");
            } else if name.ends_with(".ico") {
                let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
                let sizes: BTreeSet<u32> = (0..count)
                    .map(|i| match bytes[6 + 16 * i] {
                        0 => 256,
                        w => u32::from(w),
                    })
                    .collect();
                assert_eq!(sizes, BTreeSet::from([16, 24, 32, 48, 64, 256]), "{path}");
            } else if name.ends_with(".icns") {
                assert_eq!(&bytes[..4], b"icns", "{path}");
                let mut types = BTreeSet::new();
                let mut at = 8;
                while at + 8 <= bytes.len() {
                    let len = be_u32(&bytes, at + 4) as usize;
                    assert!(len >= 8, "{path}: broken entry");
                    types.insert(String::from_utf8_lossy(&bytes[at..at + 4]).into_owned());
                    at += len;
                }
                // 128, 256, 512 and 1024 px.
                for kind in ["ic07", "ic08", "ic09", "ic10"] {
                    assert!(types.contains(kind), "{path} lacks {kind}");
                }
            } else {
                panic!("unexpected icon {path}");
            }
        }
    }
}
