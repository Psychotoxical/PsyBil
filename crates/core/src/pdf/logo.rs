//! The logo as it goes into the PDF: never larger than its print size
//! needs, so every archived invoice stays small.

/// Pixel limit: 50 × 25 mm at about 300 dpi.
pub(crate) const MAX_W: u32 = 600;
pub(crate) const MAX_H: u32 = 300;

/// Logo bytes for the PDF. A logo within the limit is passed on as it is;
/// a larger one is scaled down and stored as PNG. `None` for anything that
/// is not a readable PNG or JPEG of that MIME type.
pub(crate) fn prepare<'a>(
    bytes: &'a [u8],
    mime: &str,
) -> Option<(std::borrow::Cow<'a, [u8]>, &'static str)> {
    use image::{ImageFormat, ImageReader};
    use std::borrow::Cow;
    use std::io::Cursor;
    let (format, mime) = match mime.trim().to_lowercase().as_str() {
        "image/png" => (ImageFormat::Png, "image/png"),
        "image/jpeg" | "image/jpg" => (ImageFormat::Jpeg, "image/jpeg"),
        _ => return None,
    };
    let reader = || {
        let mut r = ImageReader::new(Cursor::new(bytes));
        r.set_format(format);
        r
    };
    let (w, h) = reader().into_dimensions().ok()?;
    let image = reader().decode().ok()?;
    if w <= MAX_W && h <= MAX_H {
        return Some((Cow::Borrowed(bytes), mime));
    }
    let small = image.resize(MAX_W, MAX_H, image::imageops::FilterType::Triangle);
    let mut out = Cursor::new(Vec::new());
    small.write_to(&mut out, ImageFormat::Png).ok()?;
    Some((Cow::Owned(out.into_inner()), "image/png"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::pdf::render::tests::{PNG, has_image};

    /// A noisy RGBA PNG that does not compress well.
    pub(crate) fn big_png(w: u32, h: u32) -> Vec<u8> {
        let mut state: u32 = 0x1234_5678;
        let img = image::RgbaImage::from_fn(w, h, |_, _| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let [r, g, b, _] = state.to_le_bytes();
            image::Rgba([r, g, b, 255])
        });
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn small_logo_passes_unchanged() {
        let (bytes, mime) = prepare(PNG, "image/png").unwrap();
        assert_eq!((bytes.as_ref(), mime), (PNG, "image/png"));
    }

    #[test]
    fn big_logo_is_scaled_down() {
        let big = big_png(3000, 1500);
        assert!(big.len() > 2_000_000);
        let (bytes, mime) = prepare(&big, "image/png").unwrap();
        assert_eq!(mime, "image/png");
        let small = image::load_from_memory(&bytes).unwrap();
        assert!(small.width() <= MAX_W && small.height() <= MAX_H);
        assert_eq!((small.width(), small.height()), (600, 300));

        let mut doc = crate::pdf::layout::tests::sample();
        doc.number = Some("RE-2026-0001".into());
        let pdf = crate::pdf::render_invoice(&doc, Some((&big, "image/png"))).unwrap();
        assert!(has_image(&pdf));
        assert!(pdf.len() < 800_000, "pdf has {} bytes", pdf.len());
    }

    #[test]
    fn unreadable_or_other_logo_is_none() {
        assert!(prepare(&PNG[..40], "image/png").is_none());
        assert!(prepare(PNG, "image/webp").is_none());
    }
}
