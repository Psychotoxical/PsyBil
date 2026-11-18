//! File name and saving of an invoice PDF.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use uuid::Uuid;

use crate::merge::io_error;
use crate::pdf::invoice_pdf;
use crate::pdf::locale::{Labels, Lang};
use crate::pdf::model::InvoiceDoc;
use crate::{Error, Result};

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// `<number>_<customer>.pdf`, safe on every system.
pub fn pdf_file_name(doc: &InvoiceDoc) -> String {
    let number = doc.number.clone().unwrap_or_else(|| {
        Labels::of(Lang::from_code(&doc.language))
            .draft_number
            .to_string()
    });
    let raw = match doc.customer.as_ref().map(|c| c.name.trim()) {
        Some(name) if !name.is_empty() => format!("{number}_{name}"),
        _ => number,
    };
    format!("{}.pdf", safe_stem(&raw))
}

/// Writes the PDF of invoice `id` to `target` (the file the save dialog
/// chose) and returns its path. A temporary file next to it is forced onto
/// the disk and renamed, so no half-written PDF is left behind.
pub fn export_pdf(conn: &Connection, id: Uuid, target: &Path) -> Result<PathBuf> {
    let pdf = invoice_pdf(conn, id)?;
    let folder = target
        .parent()
        .filter(|p| p.is_dir())
        .ok_or_else(|| Error::Io(format!("folder of {} does not exist", target.display())))?;
    let tmp = folder.join(format!(".psybil-{}.tmp", Uuid::new_v4()));
    let written = std::fs::File::create(&tmp).and_then(|mut f| {
        use std::io::Write;
        f.write_all(&pdf)?;
        f.sync_all()
    });
    if let Err(e) = written.and_then(|()| std::fs::rename(&tmp, target)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(io_error(e));
    }
    Ok(target.to_path_buf())
}

/// Turns any text into a safe file name stem: letters, digits, `-` and `.`
/// stay, everything else becomes one `_`.
fn safe_stem(raw: &str) -> String {
    let mut stem = String::new();
    for c in raw.chars() {
        let c = if c.is_alphanumeric() || c == '-' || c == '.' {
            c
        } else {
            '_'
        };
        if !(c == '_' && stem.ends_with('_')) {
            stem.push(c);
        }
    }
    let mut stem: String = stem
        .trim_matches(|c| c == '_' || c == '.')
        .chars()
        .take(120)
        .collect();
    let base = stem.split('.').next().unwrap_or("").to_uppercase();
    if RESERVED.contains(&base.as_str()) {
        stem.push('_');
    }
    stem
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use crate::db::Db;
    use crate::invoice::create_draft;
    use crate::issue::{IssueOptions, issue_invoice};
    use crate::masterdata::{create_business, create_customer};
    use crate::pdf::archive::tests::archived;
    use crate::pdf::layout::tests::sample;
    use crate::testutil::{clock, sample_business, sample_customer, sample_draft};

    #[test]
    fn file_name_examples() {
        let mut doc = sample();
        doc.number = Some("RE-2026-0001".into());
        assert_eq!(pdf_file_name(&doc), "RE-2026-0001_Kunde_A.pdf");
        doc.customer.as_mut().unwrap().name = "Müller & Söhne / Köln".into();
        assert_eq!(pdf_file_name(&doc), "RE-2026-0001_Müller_Söhne_Köln.pdf");
        doc.customer = None;
        assert_eq!(pdf_file_name(&doc), "RE-2026-0001.pdf");
        let draft = sample();
        assert_eq!(pdf_file_name(&draft), "Entwurf_Kunde_A.pdf");
        assert_eq!(safe_stem("CON"), "CON_");
        assert_eq!(safe_stem("lpt1"), "lpt1_");
        assert_eq!(safe_stem("..Band.."), "Band");
        assert_eq!(safe_stem(&"x".repeat(300)).chars().count(), 120);
    }

    fn issued(db: &mut Db) -> Uuid {
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
        id
    }

    #[test]
    fn export_writes_archived_bytes() {
        let mut db = Db::open_in_memory().unwrap();
        let id = issued(&mut db);
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("RE-2026-0001_Kunde_A.pdf");
        let written = export_pdf(db.read(), id, &target).unwrap();
        assert_eq!(written, target);
        assert_eq!(Some(std::fs::read(&target).unwrap()), archived(&db, id).0);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn export_into_missing_folder_fails_cleanly() {
        let mut db = Db::open_in_memory().unwrap();
        let id = issued(&mut db);
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("fehlt").join("x.pdf");
        assert!(matches!(
            export_pdf(db.read(), id, &target),
            Err(Error::Io(_))
        ));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
