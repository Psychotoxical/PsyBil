//! The invoice PDF: PDF/A-3b in three templates, built in layers that can
//! be tested on their own. `model` turns a snapshot into a document,
//! `layout` places it on pages, `render` writes it with krilla.

pub(crate) mod archive;
pub mod export;
pub(crate) mod fonts;
pub(crate) mod layout;
pub(crate) mod locale;
pub(crate) mod logo;
pub mod model;
pub mod preview;
pub(crate) mod render;

pub use archive::invoice_pdf;
pub use export::{export_pdf, pdf_file_name};
pub use render::render_invoice;

#[cfg(test)]
thread_local! {
    /// Makes the next archive rendering fail, for rollback tests.
    pub(crate) static FAIL_RENDER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Millimetres to PDF points.
pub(crate) fn mm(x: f32) -> f32 {
    x * 72.0 / 25.4
}
