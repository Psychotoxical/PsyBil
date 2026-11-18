//! The embedded font (Inter, SIL Open Font License 1.1) and simple text
//! setting: shaping with rustybuzz, measuring and line breaking.

use std::sync::OnceLock;

use krilla::text::{Font, GlyphId, KrillaGlyph};
use rustybuzz::ttf_parser::Tag;
use rustybuzz::{Face, Feature, UnicodeBuffer};

static REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-SemiBold.ttf");

/// A krilla font together with its bytes, which krilla keeps private but
/// shaping and measuring need.
pub(crate) struct PdfFont {
    pub font: Font,
    face: Face<'static>,
}

impl PdfFont {
    fn load(data: &'static [u8]) -> PdfFont {
        PdfFont {
            font: Font::new(data.into(), 0).expect("embedded font loads"),
            face: Face::from_slice(data, 0).expect("embedded font parses"),
        }
    }

    pub(crate) fn units_per_em(&self) -> f32 {
        self.font.units_per_em()
    }
}

pub(crate) struct Fonts {
    pub regular: PdfFont,
    pub bold: PdfFont,
}

impl Fonts {
    pub(crate) fn get() -> &'static Fonts {
        static FONTS: OnceLock<Fonts> = OnceLock::new();
        FONTS.get_or_init(|| Fonts {
            regular: PdfFont::load(REGULAR),
            bold: PdfFont::load(BOLD),
        })
    }
}

fn with_face<T>(font: &PdfFont, f: impl FnOnce(&Face) -> T) -> T {
    f(&font.face)
}

/// The text as the font can print it: control characters except `\n` are
/// dropped, tabs become spaces, characters without a glyph become `?`.
pub(crate) fn printable(font: &PdfFont, text: &str) -> String {
    with_face(font, |face| {
        text.chars()
            .filter_map(|c| match c {
                '\n' => Some('\n'),
                '\t' => Some(' '),
                c if c.is_control() => None,
                c if face.glyph_index(c).is_none() => Some('?'),
                c => Some(c),
            })
            .collect()
    })
}

/// Glyphs of `text` in one line; advances in em like krilla's own shaping.
/// `text` must already be `printable`.
pub(crate) fn shape(font: &PdfFont, text: &str, tabular: bool) -> Vec<KrillaGlyph> {
    if !tabular {
        return shape_run(font, text, 0, false);
    }
    // Equal widths for the digits only; signs and separators keep their
    // own width, or "-29,75" would show a gap after the minus.
    let mut glyphs = Vec::new();
    let mut start = 0;
    let mut digits = text.starts_with(|c: char| c.is_ascii_digit());
    for (i, c) in text.char_indices() {
        if c.is_ascii_digit() != digits {
            glyphs.extend(shape_run(font, &text[start..i], start, digits));
            start = i;
            digits = !digits;
        }
    }
    glyphs.extend(shape_run(font, &text[start..], start, digits));
    glyphs
}

/// Shapes one run; `offset` is where the run starts in the whole text.
fn shape_run(font: &PdfFont, text: &str, offset: usize, tabular: bool) -> Vec<KrillaGlyph> {
    if text.is_empty() {
        return Vec::new();
    }
    with_face(font, |face| {
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let features = if tabular {
            vec![Feature::new(Tag::from_bytes(b"tnum"), 1, ..)]
        } else {
            Vec::new()
        };
        let output = rustybuzz::shape(face, &features, buffer);
        let upem = font.units_per_em();
        let infos = output.glyph_infos();
        let positions = output.glyph_positions();
        (0..output.len())
            .map(|i| {
                let start = infos[i].cluster as usize;
                let end = infos
                    .iter()
                    .skip(i + 1)
                    .map(|info| info.cluster as usize)
                    .find(|&c| c != start)
                    .unwrap_or(text.len());
                let pos = positions[i];
                KrillaGlyph::new(
                    GlyphId::new(infos[i].glyph_id),
                    pos.x_advance as f32 / upem,
                    pos.x_offset as f32 / upem,
                    pos.y_offset as f32 / upem,
                    pos.y_advance as f32 / upem,
                    offset + start..offset + end,
                    None,
                )
            })
            .collect()
    })
}

/// The text as one printable line: line breaks become spaces.
pub(crate) fn one_line(font: &PdfFont, text: &str) -> String {
    printable(font, text).replace('\n', " ")
}

/// Width of `text` at `size` points, in points.
pub(crate) fn text_width(font: &PdfFont, text: &str, size: f32, tabular: bool) -> f32 {
    let text = one_line(font, text);
    shape(font, &text, tabular)
        .iter()
        .map(|g| g.x_advance)
        .sum::<f32>()
        * size
}

/// Breaks `text` into lines no wider than `max_width` points: at line
/// breaks and spaces, and inside a word that is too wide on its own.
pub(crate) fn wrap(font: &PdfFont, text: &str, size: f32, max_width: f32) -> Vec<String> {
    let fits = |s: &str| text_width(font, s, size, false) <= max_width;
    let mut lines = Vec::new();
    if text.trim().is_empty() {
        return lines;
    }
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if fits(&candidate) {
                current = candidate;
                continue;
            }
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            if fits(word) {
                current = word.to_string();
                continue;
            }
            for c in word.chars() {
                let mut next = current.clone();
                next.push(c);
                if !current.is_empty() && !fits(&next) {
                    lines.push(std::mem::take(&mut current));
                    next = c.to_string();
                }
                current = next;
            }
        }
        lines.push(current);
    }
    lines
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::pdf::mm;

    const LOREM: &str = "Auftritt im Club Beispiel mit Technik, Licht und Ton, \
        inklusive Aufbau und Abbau sowie Anfahrt aus Musterstadt am Freitagabend";

    #[test]
    fn width_grows_with_text() {
        let f = &Fonts::get().regular;
        let long = text_width(f, "Rechnung", 10.0, false);
        let short = text_width(f, "Re", 10.0, false);
        assert!(long > short && short > 0.0);
    }

    #[test]
    fn tabular_digits_have_equal_width() {
        let f = &Fonts::get().regular;
        assert_eq!(
            text_width(f, "1111", 9.0, true),
            text_width(f, "8888", 9.0, true)
        );
    }

    #[test]
    fn tabular_minus_stays_narrow() {
        let f = &Fonts::get().regular;
        assert!(text_width(f, "-1", 9.0, true) < text_width(f, "11", 9.0, true));
    }

    #[test]
    fn wrap_respects_width() {
        let f = &Fonts::get().regular;
        let lines = wrap(f, LOREM, 9.0, mm(70.0));
        assert!(lines.len() > 1);
        for line in &lines {
            assert!(text_width(f, line, 9.0, false) <= mm(70.0), "{line}");
        }
        assert_eq!(lines.join(" "), LOREM);
    }

    #[test]
    fn wrap_breaks_overlong_word() {
        let f = &Fonts::get().regular;
        let word = "W".repeat(80);
        let lines = wrap(f, &word, 9.0, mm(70.0));
        assert!(lines.len() > 1);
        for line in &lines {
            assert!(text_width(f, line, 9.0, false) <= mm(70.0));
        }
        assert_eq!(lines.concat(), word);
        assert!(wrap(f, "", 9.0, mm(70.0)).is_empty());
    }

    #[test]
    fn unsupported_chars_become_question_marks() {
        let f = &Fonts::get().regular;
        assert_eq!(printable(f, "Band 🎸 東京"), "Band ? ??");
        assert_eq!(printable(f, "a\u{7}b\nc"), "ab\nc");
    }

    /// A one-page PDF/A-3b document with `text`, for checks on real output.
    pub(crate) fn one_page_pdf(text: &str) -> Vec<u8> {
        use krilla::configure::{Archival, ConfigurationBuilder};
        use krilla::geom::Point;
        use krilla::metadata::{DateTime, Metadata};
        use krilla::page::PageSettings;
        use krilla::{Document, SerializeSettings};
        let configuration = ConfigurationBuilder::new()
            .with_archival_validator(Archival::A3_B)
            .finish()
            .unwrap();
        let mut doc = Document::new_with(SerializeSettings {
            configuration,
            ..SerializeSettings::default()
        });
        doc.set_metadata(
            Metadata::new()
                .title("Test".into())
                .language("de-DE".into())
                .creation_date(DateTime::new(2026).month(11).day(18)),
        );
        let fonts = Fonts::get();
        let mut page = doc.start_page_with(PageSettings::from_wh(595.28, 841.89).unwrap());
        let mut surface = page.surface();
        let text = printable(&fonts.regular, text);
        let glyphs = shape(&fonts.regular, &text, false);
        surface.draw_glyphs(
            Point::from_xy(50.0, 100.0),
            &glyphs,
            fonts.regular.font.clone(),
            &text,
            12.0,
            false,
        );
        surface.finish();
        page.finish();
        doc.finish().unwrap()
    }

    #[test]
    fn pdfa_smoke_text_roundtrip() {
        let bytes = one_page_pdf("Rechnung äöüß € 1.234,56");
        let text = pdf_extract::extract_text_from_mem(&bytes).unwrap();
        assert!(text.contains("Rechnung äöüß € 1.234,56"), "{text:?}");
    }
}
