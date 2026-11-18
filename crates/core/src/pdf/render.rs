//! Writes the laid-out pages as PDF/A-3b with krilla.

use krilla::color::rgb;
use krilla::configure::{Archival, ConfigurationBuilder};
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::image::Image;
use krilla::metadata::{DateTime, Metadata};
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::surface::Surface;
use krilla::{Document, SerializeSettings};

use chrono::{Datelike, Timelike};

use crate::invoice::InvoiceKind;
use crate::pdf::fonts::{Fonts, one_line, shape};
use crate::pdf::layout::{Align, Op, PageOps, Rgb, layout};
use crate::pdf::locale::{Labels, Lang};
use crate::pdf::model::InvoiceDoc;
use crate::{Error, Result};

const PAGE_W: f32 = 595.28;
const PAGE_H: f32 = 841.89;

/// The PDF of `doc`. `logo` is the image bytes and MIME type (PNG or JPEG);
/// a logo that cannot be read is left out, it never stops the PDF.
pub fn render_invoice(doc: &InvoiceDoc, logo: Option<(&[u8], &str)>) -> Result<Vec<u8>> {
    let image = logo
        .and_then(|(bytes, mime)| crate::pdf::logo::prepare(bytes, mime))
        .and_then(|(bytes, mime)| load_logo(&bytes, mime));
    match (render_with(doc, image.as_ref()), &image) {
        // An image krilla only rejects while writing: try again without it.
        (Err(_), Some(_)) => render_with(doc, None),
        (result, _) => result,
    }
}

fn load_logo(bytes: &[u8], mime: &str) -> Option<Image> {
    let data = bytes.to_vec().into();
    match mime.trim().to_lowercase().as_str() {
        // PDF/A forbids image interpolation.
        "image/png" => Image::from_png(data, false).ok(),
        "image/jpeg" | "image/jpg" => Image::from_jpeg(data, false).ok(),
        _ => None,
    }
}

fn pdf_error(error: impl std::fmt::Debug) -> Error {
    Error::Pdf(format!("{error:?}"))
}

fn metadata(doc: &InvoiceDoc) -> Metadata {
    let lang = Lang::from_code(&doc.language);
    let labels = Labels::of(lang);
    let title = match doc.kind {
        InvoiceKind::Invoice => labels.title_invoice,
        InvoiceKind::Storno => labels.title_storno,
    };
    let title = match &doc.number {
        Some(number) => format!("{title} {number}"),
        None => format!("{title} ({})", labels.draft_number),
    };
    let issued = doc
        .issued_at
        .as_deref()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.naive_utc());
    let date = match issued {
        Some(t) => DateTime::new(t.year() as u16)
            .month(t.month() as u8)
            .day(t.day() as u8)
            .hour(t.hour() as u8)
            .minute(t.minute() as u8)
            .second(t.second() as u8)
            .utc_offset_hour(0)
            .utc_offset_minute(0),
        None => DateTime::new(doc.invoice_date.year() as u16)
            .month(doc.invoice_date.month() as u8)
            .day(doc.invoice_date.day() as u8),
    };
    Metadata::new()
        .title(title)
        .authors(vec![doc.business.name.clone()])
        .language(
            match lang {
                Lang::De => "de-DE",
                Lang::En => "en-GB",
            }
            .into(),
        )
        .creator("PsyBil".into())
        .creation_date(date)
}

fn render_with(doc: &InvoiceDoc, logo: Option<&Image>) -> Result<Vec<u8>> {
    let pages = layout(doc, logo.map(Image::size));
    write_pages(&pages, metadata(doc), logo)
}

/// Writes laid-out A4 pages as a PDF/A-3b document.
pub(crate) fn write_pages(
    pages: &[PageOps],
    metadata: Metadata,
    logo: Option<&Image>,
) -> Result<Vec<u8>> {
    let configuration = ConfigurationBuilder::new()
        .with_archival_validator(Archival::A3_B)
        .finish()
        .map_err(pdf_error)?;
    let mut document = Document::new_with(SerializeSettings {
        configuration,
        ..SerializeSettings::default()
    });
    document.set_metadata(metadata);
    let settings = PageSettings::from_wh(PAGE_W, PAGE_H).ok_or_else(|| pdf_error("page size"))?;
    for page_ops in pages {
        let mut page = document.start_page_with(settings.clone());
        let mut surface = page.surface();
        draw_page(&mut surface, page_ops, logo);
        surface.finish();
        page.finish();
    }
    document.finish().map_err(pdf_error)
}

fn color(c: Rgb) -> rgb::Color {
    rgb::Color::new(c.0, c.1, c.2)
}

fn fill(c: Rgb) -> Fill {
    Fill {
        paint: color(c).into(),
        ..Fill::default()
    }
}

/// Draws one line of text with its start at (x, baseline y).
#[allow(clippy::too_many_arguments)]
fn draw_text(
    surface: &mut Surface,
    x: f32,
    y: f32,
    size: f32,
    bold: bool,
    tabular: bool,
    c: Rgb,
    align: Align,
    text: &str,
) {
    let fonts = Fonts::get();
    let font = if bold { &fonts.bold } else { &fonts.regular };
    let text = one_line(font, text);
    let glyphs = shape(font, &text, tabular);
    let width: f32 = glyphs.iter().map(|g| g.x_advance).sum::<f32>() * size;
    let start = match align {
        Align::Left => x,
        Align::Right => x - width,
    };
    surface.set_fill(Some(fill(c)));
    surface.set_stroke(None);
    surface.draw_glyphs(
        Point::from_xy(start, y),
        &glyphs,
        font.font.clone(),
        &text,
        size,
        false,
    );
}

fn draw_page(surface: &mut Surface, page: &PageOps, logo: Option<&Image>) {
    for op in &page.ops {
        match op {
            Op::Text {
                x,
                y,
                size,
                bold,
                tabular,
                color,
                align,
                text,
            } => draw_text(
                surface, *x, *y, *size, *bold, *tabular, *color, *align, text,
            ),
            Op::Rect {
                x,
                y,
                w,
                h,
                fill: c,
            } => {
                let mut path = PathBuilder::new();
                if let Some(rect) = Rect::from_xywh(*x, *y, *w, *h) {
                    path.push_rect(rect);
                }
                if let Some(path) = path.finish() {
                    surface.set_fill(Some(fill(*c)));
                    surface.set_stroke(None);
                    surface.draw_path(&path);
                }
            }
            Op::Line {
                x1,
                y1,
                x2,
                y2,
                width,
                color: c,
            } => {
                let mut path = PathBuilder::new();
                path.move_to(*x1, *y1);
                path.line_to(*x2, *y2);
                if let Some(path) = path.finish() {
                    surface.set_fill(None);
                    surface.set_stroke(Some(Stroke {
                        paint: color(*c).into(),
                        width: *width,
                        ..Stroke::default()
                    }));
                    surface.draw_path(&path);
                }
            }
            Op::Logo { x, y, w, h } => {
                if let (Some(image), Some(size)) = (logo, Size::from_wh(*w, *h)) {
                    surface.push_transform(&Transform::from_translate(*x, *y));
                    surface.draw_image(image.clone(), size);
                    surface.pop();
                }
            }
            Op::Watermark { text } => {
                let size = 72.0;
                let fonts = Fonts::get();
                let shown = one_line(&fonts.bold, text);
                let width: f32 = shape(&fonts.bold, &shown, false)
                    .iter()
                    .map(|g| g.x_advance)
                    .sum::<f32>()
                    * size;
                surface.push_transform(&Transform::from_translate(PAGE_W / 2.0, PAGE_H / 2.0));
                surface.push_transform(&Transform::from_rotate(-45.0));
                draw_text(
                    surface,
                    -width / 2.0,
                    size * 0.35,
                    size,
                    true,
                    false,
                    Rgb(230, 230, 230),
                    Align::Left,
                    &shown,
                );
                surface.pop();
                surface.pop();
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::Db;
    use crate::invoice::create_draft;
    use crate::issue::{IssueOptions, issue_invoice};
    use crate::masterdata::{create_business, create_customer};
    use crate::pdf::layout::tests::sample;
    use crate::pdf::model::issued_doc;
    use crate::storno::cancel_invoice;
    use crate::testutil::{clock, sample_business, sample_customer, sample_draft};

    pub(crate) const PNG: &[u8] = include_bytes!("testdata/logo-alpha.png");
    pub(crate) const JPEG: &[u8] = include_bytes!("testdata/logo.jpg");

    pub(crate) fn text_of(pdf: &[u8]) -> String {
        pdf_extract::extract_text_from_mem(pdf).unwrap()
    }

    pub(crate) fn has_image(pdf: &[u8]) -> bool {
        let doc = lopdf::Document::load_mem(pdf).unwrap();
        doc.objects.values().any(|object| {
            object.as_stream().is_ok_and(|s| {
                s.dict
                    .get(b"Subtype")
                    .and_then(|v| v.as_name())
                    .is_ok_and(|n| n == b"Image")
            })
        })
    }

    fn issued() -> InvoiceDoc {
        let mut doc = sample();
        doc.number = Some("RE-2026-0001".into());
        doc.issued_at = Some("2026-11-18T10:00:00.000Z".into());
        doc
    }

    #[test]
    fn renders_valid_pdfa3b_with_mandatory_fields() {
        let pdf = render_invoice(&issued(), None).unwrap();
        let text = text_of(&pdf);
        for needed in [
            "Beispiel GbR",
            "Musterstraße 1",
            "12345 Musterstadt",
            "12/345/67890",
            "Kunde A",
            "Hauptstraße 2",
            "54321 Beispielstadt",
            "RE-2026-0001",
            "18.11.2026",
            "entspricht Rechnungsdatum",
            "Leistung",
            "25,00 €",
            "19 %",
            "4,75 €",
            "29,75 €",
        ] {
            assert!(text.contains(needed), "missing {needed:?} in\n{text}");
        }
        assert!(!has_image(&pdf));
    }

    #[test]
    fn same_input_same_bytes() {
        let doc = issued();
        assert_eq!(
            render_invoice(&doc, Some((PNG, "image/png"))).unwrap(),
            render_invoice(&doc, Some((PNG, "image/png"))).unwrap()
        );
    }

    #[test]
    fn png_with_alpha_renders_pdfa() {
        let pdf = render_invoice(&issued(), Some((PNG, "image/png"))).unwrap();
        assert!(has_image(&pdf));
    }

    #[test]
    fn jpeg_logo_renders() {
        let pdf = render_invoice(&issued(), Some((JPEG, "image/jpeg"))).unwrap();
        assert!(has_image(&pdf));
    }

    #[test]
    fn broken_logo_is_left_out() {
        let pdf = render_invoice(&issued(), Some((&PNG[..40], "image/png"))).unwrap();
        assert!(!has_image(&pdf));
        let pdf = render_invoice(&issued(), Some((PNG, "image/webp"))).unwrap();
        assert!(!has_image(&pdf));
    }

    #[test]
    fn emoji_customer_name_still_valid_pdfa() {
        let mut doc = issued();
        doc.customer.as_mut().unwrap().name = "Band 🎸".into();
        let pdf = render_invoice(&doc, None).unwrap();
        assert!(text_of(&pdf).contains("Band ?"));
    }

    #[test]
    fn storno_shows_reference_and_negative_total() {
        let mut db = Db::open_in_memory().unwrap();
        let (_, brand) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let customer = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        let id = db
            .write(|tx| create_draft(tx, &clock(), sample_draft(brand, Some(customer))))
            .unwrap();
        db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()))
            .unwrap();
        let storno = db.write(|tx| cancel_invoice(tx, &clock(), id)).unwrap();
        let pdf = render_invoice(&issued_doc(db.read(), storno).unwrap(), None).unwrap();
        let text = text_of(&pdf);
        for needed in [
            "Stornorechnung",
            "Storno zu",
            "RE-2026-0001 vom 18.11.2026",
            "-29,75 €",
        ] {
            assert!(text.contains(needed), "missing {needed:?} in\n{text}");
        }
    }

    /// Writes sample PDFs into `$PSYBIL_SAMPLES` for a look by eye.
    #[test]
    #[ignore]
    fn write_samples() {
        let dir = std::path::PathBuf::from(std::env::var("PSYBIL_SAMPLES").unwrap());
        std::fs::create_dir_all(&dir).unwrap();
        let dress = |doc: &mut InvoiceDoc| {
            doc.brand.iban = "DE12 3456 7890 1234 5678 90".into();
            doc.brand.bic = "BEISDEXXX".into();
            doc.brand.bank_name = "Bank Beispiel".into();
            doc.brand.email = "post@beispiel.example".into();
            doc.brand.phone = "0123 456789".into();
            doc.brand.website = "www.beispiel.example".into();
            doc.brand.color = "#1e66f5".into();
            doc.intro = "Vielen Dank für den Auftrag. Wir berechnen wie vereinbart:".into();
            doc.outro = "Bei Fragen zur Rechnung erreichen Sie uns unter der unten \
                         angegebenen Adresse."
                .into();
        };
        let save = |name: &str, doc: &InvoiceDoc| {
            let pdf = render_invoice(doc, Some((PNG, "image/png"))).unwrap();
            std::fs::write(dir.join(format!("{name}.pdf")), pdf).unwrap();
        };
        let mut doc = issued();
        dress(&mut doc);
        for (template, name) in [
            ("classic", "1-klassisch"),
            ("modern", "2-modern"),
            ("minimal", "3-minimal"),
        ] {
            doc.template = template.into();
            save(name, &doc);
        }
        doc.template = "classic".into();

        let mut long = doc.clone();
        let template_line = long.lines[0].clone();
        long.lines.clear();
        for i in 0..48 {
            let mut line = template_line.clone();
            line.description = match i % 4 {
                0 => format!("Auftritt {}, inklusive Technik und Anfahrt", i / 4 + 1),
                1 => "Shirt, Größe M".into(),
                2 => "Tontechnik vor Ort mit Aufbau und Abbau sowie Bereitstellung \
                      der Monitoranlage für die gesamte Veranstaltung"
                    .into(),
                _ => "Plakat A2".into(),
            };
            long.lines.push(line);
        }
        let inputs: Vec<_> = long
            .lines
            .iter()
            .map(crate::invoice::LineDraft::to_input)
            .collect();
        long.totals = crate::calc::calculate(&inputs, crate::calc::PriceMode::Net, false).unwrap();
        save("4-mehrseitig", &long);

        let mut storno = doc.clone();
        storno.kind = InvoiceKind::Storno;
        storno.number = Some("ST-2026-0001".into());
        storno.cancels = Some(("RE-2026-0001".into(), storno.invoice_date));
        storno.totals = storno.totals.negated();
        storno.intro = String::new();
        for line in &mut storno.lines {
            line.quantity = -line.quantity;
        }
        save("5-storno", &storno);

        let mut english = doc.clone();
        english.language = "en".into();
        english.intro = "Thank you for your order. As agreed, we invoice:".into();
        english.outro = "If you have any questions, please contact us at the address below.".into();
        save("6-englisch", &english);

        let mut small = crate::pdf::layout::tests::doc_with(|b, _, _| b.small_business = true);
        small.number = Some("RE-2026-0002".into());
        small.issued_at = doc.issued_at.clone();
        dress(&mut small);
        small.payment_received = true;
        save("7-kleinunternehmer-bezahlt", &small);

        let mut replacement = doc.clone();
        replacement.number = Some("RE-2026-0003".into());
        replacement.paid_before = crate::money::Cents(1000);
        save("8-ersatz-teilbezahlt", &replacement);
    }

    #[test]
    fn newline_in_single_line_fields_still_renders() {
        let mut doc = issued();
        doc.brand.name = "Band\nName".into();
        doc.lines[0].unit = "Stk\nx".into();
        doc.number = Some("RE-1\n2".into());
        doc.currency = "EU\nR".into();
        doc.totals.groups[0].category = crate::tax::TaxCategory::ReverseCharge;
        doc.customer.as_mut().unwrap().vat_id = "AT\nU123".into();
        doc.business.tax_number = "12/345\n67890".into();
        let pdf = render_invoice(&doc, None);
        assert!(pdf.is_ok(), "{:?}", pdf.err());
        assert!(text_of(&pdf.unwrap()).contains("Band Name"));
    }

    #[test]
    fn all_templates_render_valid_pdfa() {
        for template in ["classic", "modern", "minimal"] {
            let mut doc = issued();
            doc.template = template.into();
            doc.brand.color = "#3b82f6".into();
            assert!(
                render_invoice(&doc, Some((PNG, "image/png"))).is_ok(),
                "{template}"
            );
        }
    }

    #[test]
    fn english_invoice() {
        let mut doc = issued();
        doc.language = "en".into();
        let text = text_of(&render_invoice(&doc, None).unwrap());
        for needed in ["Invoice", "18 Nov 2026", "€29.75"] {
            assert!(text.contains(needed), "missing {needed:?} in\n{text}");
        }
    }
}
