//! The preview of an invoice as pictures: the real PDF, rasterised in the
//! core, so it looks the same on every system.

use rusqlite::Connection;
use uuid::Uuid;

use crate::Result;

/// One PNG per page of the invoice PDF, `width_px` wide.
pub fn preview_png(conn: &Connection, id: Uuid, width_px: u32) -> Result<Vec<Vec<u8>>> {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::hayro_syntax::Pdf;
    use hayro::vello_cpu::color::palette::css::WHITE;
    use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
    let bytes = crate::pdf::invoice_pdf(conn, id)?;
    let pdf = Pdf::new(std::sync::Arc::new(bytes))
        .map_err(|e| crate::Error::Pdf(format!("preview: {e:?}")))?;
    let cache = RenderCache::new();
    let mut pages = Vec::new();
    for page in pdf.pages().iter() {
        let (width, _) = page.render_dimensions();
        // A half pixel more, so rounding down still gives `width_px`.
        let scale = (width_px as f32 + 0.5) / width;
        let pixmap = render(
            page,
            &cache,
            &InterpreterSettings::default(),
            &RenderSettings::default(),
            &PixmapSettings {
                x_scale: scale,
                y_scale: scale,
                bg_color: WHITE,
            },
        );
        let png = pixmap
            .into_png()
            .map_err(|e| crate::Error::Pdf(format!("preview: {e}")))?;
        pages.push(png);
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::invoice::create_draft;
    use crate::masterdata::{create_business, create_customer};
    use crate::testutil::{clock, sample_business, sample_customer, sample_draft};

    fn draft(lines: usize) -> (Db, Uuid) {
        let mut db = Db::open_in_memory().unwrap();
        let (_, brand) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let customer = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        let mut input = sample_draft(brand, Some(customer));
        let line = input.lines[0].clone();
        for i in 1..lines {
            let mut more = line.clone();
            more.description = format!("Leistung {i}");
            input.lines.push(more);
        }
        let id = db.write(|tx| create_draft(tx, &clock(), input)).unwrap();
        (db, id)
    }

    /// Writes a preview picture into `$PSYBIL_SAMPLES` for a look by eye.
    #[test]
    #[ignore]
    fn write_preview_sample() {
        let dir = std::path::PathBuf::from(std::env::var("PSYBIL_SAMPLES").unwrap());
        std::fs::create_dir_all(&dir).unwrap();
        let (db, id) = draft(3);
        let pages = preview_png(db.read(), id, 800).unwrap();
        std::fs::write(dir.join("vorschau.png"), &pages[0]).unwrap();
    }

    #[test]
    fn preview_has_one_png_per_page() {
        let (db, id) = draft(1);
        let pages = preview_png(db.read(), id, 800).unwrap();
        assert_eq!(pages.len(), 1);
        assert!(pages[0].starts_with(b"\x89PNG\r\n\x1a\n"));
        let img = image::load_from_memory(&pages[0]).unwrap();
        assert_eq!(img.width(), 800);
        assert!(img.height() > 1100 && img.height() < 1150, "A4 ratio");

        let (db, id) = draft(60);
        assert!(preview_png(db.read(), id, 400).unwrap().len() >= 2);
    }
}
