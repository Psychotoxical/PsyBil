//! The archive: an issued invoice keeps the PDF it was issued with, byte
//! for byte. A reprint always returns those bytes.

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use uuid::Uuid;

use crate::Result;
use crate::invoice::{InvoiceState, load_invoice};
use crate::issue::sha256_hex;
use crate::masterdata::get_brand;
use crate::pdf::model::{InvoiceDoc, draft_doc, issued_doc};
use crate::pdf::render_invoice;

/// The brand logo as of now; for a frozen document only when it is still
/// the logo the snapshot names.
fn render(conn: &Connection, doc: &InvoiceDoc, frozen: bool) -> Result<Vec<u8>> {
    let brand = get_brand(conn, doc.brand.id).ok();
    let logo = brand.as_ref().and_then(|b| {
        let (bytes, mime) = (b.logo.as_deref()?, b.logo_mime.as_deref()?);
        let current = sha256_hex(bytes);
        (!frozen || doc.logo_sha256.as_deref() == Some(current.as_str())).then_some((bytes, mime))
    });
    render_invoice(doc, logo)
}

/// Renders the PDF of the freshly frozen invoice `id` and stores it with
/// its hash, once.
pub(crate) fn archive_pdf(tx: &Transaction, id: Uuid) -> Result<()> {
    #[cfg(test)]
    if crate::pdf::FAIL_RENDER.with(|f| f.get()) {
        return Err(crate::Error::Pdf("forced for a test".into()));
    }
    let doc = issued_doc(tx, id)?;
    let pdf = render(tx, &doc, true)?;
    let hash = sha256_hex(&pdf);
    tx.execute(
        "UPDATE invoice_snapshots SET pdf = ?2, pdf_sha256 = ?3
         WHERE invoice_id = ?1 AND pdf IS NULL",
        params![id.to_string(), pdf, hash],
    )?;
    Ok(())
}

/// The PDF of an invoice: the archived bytes when issued, else a fresh
/// rendering of the draft.
pub fn invoice_pdf(conn: &Connection, id: Uuid) -> Result<Vec<u8>> {
    let invoice = load_invoice(conn, id)?;
    if invoice.state == InvoiceState::Draft {
        return render(conn, &draft_doc(conn, id)?, false);
    }
    let archived: Option<Vec<u8>> = conn
        .query_row(
            "SELECT pdf FROM invoice_snapshots WHERE invoice_id = ?1",
            [id.to_string()],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    match archived {
        Some(pdf) => Ok(pdf),
        // An older snapshot without archive: render it, without storing.
        None => render(conn, &issued_doc(conn, id)?, true),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::Error;
    use crate::db::Db;
    use crate::invoice::{InvoiceState, create_draft, load_invoice};
    use crate::issue::{IssueOptions, issue_invoice, sha256_hex};
    use crate::masterdata::{
        create_business, create_customer, get_brand, get_business, update_brand, update_business,
    };
    use crate::pdf::render::tests::{JPEG, PNG, text_of};
    use crate::storno::cancel_invoice;
    use crate::testutil::{clock, sample_business, sample_customer, sample_draft};

    pub(crate) fn archived(db: &Db, id: Uuid) -> (Option<Vec<u8>>, Option<String>) {
        db.read()
            .query_row(
                "SELECT pdf, pdf_sha256 FROM invoice_snapshots WHERE invoice_id = ?1",
                [id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
    }

    /// A business with a PNG logo and one draft.
    fn setup(db: &mut Db) -> (Uuid, Uuid, Uuid) {
        let (business, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let mut brand = get_brand(db.read(), brand_id).unwrap();
        brand.logo = Some(PNG.to_vec());
        brand.logo_mime = Some("image/png".into());
        db.write(|tx| update_brand(tx, &clock(), &brand)).unwrap();
        let customer = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        let id = db
            .write(|tx| create_draft(tx, &clock(), sample_draft(brand_id, Some(customer))))
            .unwrap();
        (business, brand_id, id)
    }

    fn issue(db: &mut Db, id: Uuid) -> Result<crate::issue::Issued> {
        db.write(|tx| issue_invoice(tx, &clock(), id, IssueOptions::default()))
    }

    #[test]
    fn issue_archives_pdf_with_hash() {
        let mut db = Db::open_in_memory().unwrap();
        let (_, _, id) = setup(&mut db);
        issue(&mut db, id).unwrap();
        let (pdf, hash) = archived(&db, id);
        let pdf = pdf.expect("archived pdf");
        assert_eq!(hash.as_deref(), Some(sha256_hex(&pdf).as_str()));
        assert!(text_of(&pdf).contains("RE-2026-0001"));
        assert!(crate::pdf::render::tests::has_image(&pdf));
    }

    #[test]
    fn reprint_returns_archived_bytes_after_brand_change() {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id, id) = setup(&mut db);
        issue(&mut db, id).unwrap();
        let mut brand = get_brand(db.read(), brand_id).unwrap();
        brand.logo = Some(JPEG.to_vec());
        brand.logo_mime = Some("image/jpeg".into());
        brand.pdf_template = "modern".into();
        db.write(|tx| update_brand(tx, &clock(), &brand)).unwrap();
        let mut business = get_business(db.read(), business_id).unwrap();
        business.street = "Neue Straße 9".into();
        db.write(|tx| update_business(tx, &clock(), &business))
            .unwrap();
        let reprint = invoice_pdf(db.read(), id).unwrap();
        assert_eq!(Some(reprint), archived(&db, id).0);
    }

    #[test]
    fn storno_is_archived() {
        let mut db = Db::open_in_memory().unwrap();
        let (_, _, id) = setup(&mut db);
        issue(&mut db, id).unwrap();
        let storno = db.write(|tx| cancel_invoice(tx, &clock(), id)).unwrap();
        let pdf = archived(&db, storno).0.expect("archived storno");
        assert!(text_of(&pdf).contains("Stornorechnung"));
    }

    #[test]
    fn failed_render_rolls_back_issue() {
        let mut db = Db::open_in_memory().unwrap();
        let (_, _, id) = setup(&mut db);
        crate::pdf::FAIL_RENDER.with(|f| f.set(true));
        let result = issue(&mut db, id);
        crate::pdf::FAIL_RENDER.with(|f| f.set(false));
        assert!(matches!(result, Err(Error::Pdf(_))));
        let invoice = load_invoice(db.read(), id).unwrap();
        assert_eq!((invoice.state, invoice.number), (InvoiceState::Draft, None));
        assert_eq!(issue(&mut db, id).unwrap().number, "RE-2026-0001");
    }

    #[test]
    fn draft_pdf_has_watermark() {
        let mut db = Db::open_in_memory().unwrap();
        let (_, _, id) = setup(&mut db);
        let text = text_of(&invoice_pdf(db.read(), id).unwrap());
        assert!(text.contains("ENTWURF"));
        assert!(text.contains("Entwurf"));
    }
}
