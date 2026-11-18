//! The PDF that was sent at the time, attached to an invoice taken over
//! from VibeBill. Attachments are only ever added, never changed.

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use uuid::Uuid;

use crate::dates::{Clock, timestamp};
use crate::invoice::{InvoiceState, load_invoice};
use crate::{Error, Result};

fn refuse() -> Error {
    Error::Validation(Vec::new())
}

/// Attaches `bytes` (a PDF) to the imported, issued invoice `invoice_id`.
pub fn attach_original_pdf(
    tx: &Transaction,
    clock: &dyn Clock,
    invoice_id: Uuid,
    file_name: &str,
    bytes: &[u8],
) -> Result<Uuid> {
    let invoice = load_invoice(tx, invoice_id)?;
    if !invoice.imported || invoice.state != InvoiceState::Issued || !bytes.starts_with(b"%PDF-") {
        return Err(refuse());
    }
    let id = Uuid::new_v4();
    tx.execute(
        "INSERT INTO invoice_attachments (id, invoice_id, kind, file_name, bytes, created_at)
         VALUES (?1, ?2, 'original_pdf', ?3, ?4, ?5)",
        params![
            id.to_string(),
            invoice_id.to_string(),
            file_name,
            bytes,
            timestamp(clock)
        ],
    )?;
    crate::audit::log(
        tx,
        clock,
        "invoice",
        invoice_id,
        "attach_original_pdf",
        serde_json::json!({ "file_name": file_name, "size": bytes.len() }),
    )?;
    Ok(id)
}

/// The newest original PDF of `invoice_id`: file name and bytes.
pub fn original_pdf(conn: &Connection, invoice_id: Uuid) -> Result<Option<(String, Vec<u8>)>> {
    Ok(conn
        .query_row(
            "SELECT file_name, bytes FROM invoice_attachments
             WHERE invoice_id = ?1 AND kind = 'original_pdf'
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
            [invoice_id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::legacy::{apply_import, plan_import, read_legacy, testdata::full_db};
    use crate::testutil::{ManualClock, clock};

    const PDF: &[u8] = b"%PDF-1.4 old invoice";

    /// A database with the synthetic VibeBill data taken over; returns an
    /// imported, issued invoice.
    fn imported(db: &mut Db) -> Uuid {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vibebill.db");
        full_db(&path);
        let data = read_legacy(&path, dir.path()).unwrap();
        let plan = plan_import(db.read(), &data).unwrap();
        db.write(|tx| apply_import(tx, &clock(), &data, &plan))
            .unwrap();
        let id: String = db
            .read()
            .query_row(
                "SELECT id FROM invoices WHERE imported = 1 AND state = 'issued' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        id.parse().unwrap()
    }

    #[test]
    fn attach_original_pdf_only_for_imported_pdf() {
        let mut db = Db::open_in_memory().unwrap();
        let invoice = imported(&mut db);
        assert!(matches!(
            db.write(|tx| attach_original_pdf(tx, &clock(), invoice, "alt.png", b"\x89PNG")),
            Err(Error::Validation(_))
        ));
        assert_eq!(original_pdf(db.read(), invoice).unwrap(), None);
        db.write(|tx| attach_original_pdf(tx, &clock(), invoice, "RE-alt.pdf", PDF))
            .unwrap();
        assert_eq!(
            original_pdf(db.read(), invoice).unwrap(),
            Some(("RE-alt.pdf".into(), PDF.to_vec()))
        );
        // A draft is not an imported invoice that was sent.
        let draft: String = db
            .read()
            .query_row(
                "SELECT id FROM invoices WHERE state = 'draft' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(matches!(
            db.write(|tx| attach_original_pdf(tx, &clock(), draft.parse().unwrap(), "x.pdf", PDF)),
            Err(Error::Validation(_))
        ));
    }

    #[test]
    fn original_pdf_survives_merge() {
        let dir = tempfile::tempdir().unwrap();
        let (pa, pb) = (dir.path().join("a.db"), dir.path().join("b.db"));
        let mut a = Db::open(&pa).unwrap();
        let invoice = imported(&mut a);
        a.read()
            .execute("VACUUM INTO ?1", [pb.to_string_lossy().to_string()])
            .unwrap();
        a.write(|tx| attach_original_pdf(tx, &clock(), invoice, "RE-alt.pdf", PDF))
            .unwrap();
        drop(a);
        let mut b = Db::open(&pb).unwrap();
        b.merge_from(&ManualClock::new(), &pa, None).unwrap();
        assert_eq!(
            original_pdf(b.read(), invoice).unwrap().map(|(n, _)| n),
            Some("RE-alt.pdf".into())
        );
    }
}
