/// Every command the window may call. Each gets one permission
/// (`allow-<name>`), granted in `capabilities/main.json`.
const COMMANDS: &[&str] = &[
    "frontend_ready",
    "load_settings",
    "save_settings",
    "list_businesses",
    "create_business",
    "update_business",
    "archive_business",
    "delete_business",
    "list_brands",
    "create_brand",
    "update_brand",
    "set_brand_logo",
    "remove_brand_logo",
    "brand_logo",
    "archive_brand",
    "delete_brand",
    "list_customers",
    "create_customer",
    "update_customer",
    "archive_customer",
    "delete_customer",
    "list_products",
    "create_product",
    "update_product",
    "delete_product",
    "list_invoices",
    "load_invoice",
    "create_draft",
    "save_draft",
    "delete_draft",
    "draft_totals",
    "product_line",
    "check_issue",
    "peek_number",
    "issue_invoice",
    "cancel_invoice",
    "correct_invoice",
    "duplicate_as_draft",
    "invoice_status",
    "list_payments",
    "add_payment",
    "delete_payment",
    "preview_png",
    "pdf_file_name",
    "export_pdf",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri build");
}
