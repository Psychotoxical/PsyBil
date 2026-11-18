//! Finding and reading VibeBill databases. Missing columns of early
//! versions get the defaults the old app used.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use base64::Engine;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Result;
use crate::merge::io_error;

/// The app identifier VibeBill used; its data lives in a folder of this name.
pub const LEGACY_IDENTIFIER: &str = "de.stelle.vibebill";

/// Where VibeBill kept its data on this system: `%APPDATA%` on Windows,
/// `~/.config` on Linux, `~/Library/Application Support` on macOS.
pub fn default_config_dir() -> Option<PathBuf> {
    dirs::config_dir()
}

/// VibeBill database files below `config_dir`.
pub fn find_legacy_databases(config_dir: &Path) -> Vec<PathBuf> {
    let dir = config_dir.join(LEGACY_IDENTIFIER);
    ["vibebill.db", "rechnung.db"]
        .iter()
        .map(|name| dir.join(name))
        .filter(|p| p.is_file())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacySeller {
    pub id: i64,
    pub name: String,
    pub first_name: String,
    pub last_name: String,
    pub street: String,
    pub city: String,
    pub zip: String,
    pub country: String,
    pub phone: String,
    pub email: String,
    pub website: String,
    pub tax_id: String,
    pub vat_id: String,
    pub bank_name: String,
    pub bank_iban: String,
    pub bank_bic: String,
    /// Logo bytes and MIME type.
    pub logo: Option<(Vec<u8>, String)>,
    pub invoice_prefix: String,
    pub next_invoice_number: i64,
    pub pdf_template: String,
    pub color: String,
    pub default_payment_terms: String,
    pub default_tax_rate: Option<f64>,
    pub currency: String,
    pub default_note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyCustomer {
    pub id: i64,
    pub name: String,
    pub street: String,
    pub city: String,
    pub zip: String,
    pub country: String,
    pub phone: String,
    pub email: String,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyProduct {
    pub id: i64,
    pub seller_id: i64,
    pub name: String,
    pub description: String,
    /// "product" or "service".
    pub kind: String,
    pub unit: String,
    pub price_net: f64,
    pub tax_rate: f64,
    pub stock: i64,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyItem {
    pub product_id: Option<i64>,
    pub position: i64,
    pub description: String,
    pub quantity: f64,
    pub unit: String,
    pub price_net: f64,
    pub tax_rate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyInvoice {
    pub id: i64,
    pub seller_id: i64,
    pub customer_id: i64,
    pub invoice_number: String,
    pub date: String,
    pub due_date: String,
    pub status: String,
    pub notes: String,
    pub payment_terms: String,
    pub total_net: f64,
    pub total_tax: f64,
    pub total_gross: f64,
    pub shipping_net: f64,
    pub shipping_tax_rate: f64,
    pub already_paid: bool,
    pub items: Vec<LegacyItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyPayment {
    pub id: i64,
    pub invoice_id: i64,
    pub amount: f64,
    pub date: String,
    pub method: String,
    pub notes: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LegacyData {
    pub sellers: Vec<LegacySeller>,
    pub customers: Vec<LegacyCustomer>,
    pub products: Vec<LegacyProduct>,
    pub invoices: Vec<LegacyInvoice>,
    pub payments: Vec<LegacyPayment>,
    pub warnings: Vec<String>,
}

/// Decodes a `data:<mime>;base64,<data>` URL.
pub fn decode_data_url(url: &str) -> Option<(Vec<u8>, String)> {
    let rest = url.strip_prefix("data:")?;
    let (header, data) = rest.split_once(',')?;
    let mime = header.strip_suffix(";base64")?;
    if mime.is_empty() {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .ok()?;
    Some((bytes, mime.to_string()))
}

fn table_columns(conn: &Connection, table: &str) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
    Ok(names.collect::<rusqlite::Result<_>>()?)
}

/// A column, or its default when this version lacks it.
fn col(cols: &HashSet<String>, name: &str, default: &str) -> String {
    if cols.contains(name) {
        format!("COALESCE({name}, {default})")
    } else {
        default.to_string()
    }
}

fn select(
    conn: &Connection,
    table: &str,
    spec: &[(&str, &str)],
    tail: &str,
) -> Result<Option<String>> {
    let cols = table_columns(conn, table)?;
    if cols.is_empty() {
        return Ok(None);
    }
    let list = spec
        .iter()
        .map(|(name, default)| col(&cols, name, default))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(Some(format!("SELECT {list} FROM {table} {tail}")))
}

/// Reads a VibeBill database. Only a copy in `scratch` is opened.
pub fn read_legacy(path: &Path, scratch: &Path) -> Result<LegacyData> {
    std::fs::create_dir_all(scratch).map_err(io_error)?;
    let copy = scratch.join(format!("vibebill-{}.db", Uuid::new_v4()));
    std::fs::copy(path, &copy).map_err(io_error)?;
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    if wal.is_file() {
        std::fs::copy(&wal, format!("{}-wal", copy.display())).map_err(io_error)?;
    }
    let result = {
        let conn = Connection::open(&copy)?;
        read_all(&conn)
    };
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", copy.display()));
    }
    result
}

fn read_all(conn: &Connection) -> Result<LegacyData> {
    let mut data = LegacyData::default();
    if let Some(sql) = select(
        conn,
        "sellers",
        &[
            ("id", "0"),
            ("name", "''"),
            ("first_name", "''"),
            ("last_name", "''"),
            ("street", "''"),
            ("city", "''"),
            ("zip", "''"),
            ("country", "''"),
            ("phone", "''"),
            ("email", "''"),
            ("website", "''"),
            ("tax_id", "''"),
            ("vat_id", "''"),
            ("bank_name", "''"),
            ("bank_iban", "''"),
            ("bank_bic", "''"),
            ("logo_data", "''"),
            ("invoice_prefix", "'RE'"),
            ("next_invoice_number", "1"),
            ("pdf_template", "'classic'"),
            ("color", "'#3b82f6'"),
            ("default_payment_terms", "''"),
            ("default_tax_rate", "NULL"),
            ("currency", "''"),
            ("default_note", "''"),
        ],
        "ORDER BY id",
    )? {
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok((
                LegacySeller {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    first_name: r.get(2)?,
                    last_name: r.get(3)?,
                    street: r.get(4)?,
                    city: r.get(5)?,
                    zip: r.get(6)?,
                    country: r.get(7)?,
                    phone: r.get(8)?,
                    email: r.get(9)?,
                    website: r.get(10)?,
                    tax_id: r.get(11)?,
                    vat_id: r.get(12)?,
                    bank_name: r.get(13)?,
                    bank_iban: r.get(14)?,
                    bank_bic: r.get(15)?,
                    logo: None,
                    invoice_prefix: r.get(17)?,
                    next_invoice_number: r.get(18)?,
                    pdf_template: r.get(19)?,
                    color: r.get(20)?,
                    default_payment_terms: r.get(21)?,
                    default_tax_rate: r.get(22)?,
                    currency: r.get(23)?,
                    default_note: r.get(24)?,
                },
                r.get::<_, String>(16)?,
            ))
        })?;
        for row in rows {
            let (mut seller, logo) = row?;
            if !logo.trim().is_empty() {
                seller.logo = decode_data_url(&logo);
                if seller.logo.is_none() {
                    data.warnings
                        .push(format!("Logo von {} ließ sich nicht lesen", seller.name));
                }
            }
            if seller.currency.trim().is_empty() {
                seller.currency = "EUR".into();
            }
            data.sellers.push(seller);
        }
    }
    if let Some(sql) = select(
        conn,
        "customers",
        &[
            ("id", "0"),
            ("name", "''"),
            ("street", "''"),
            ("city", "''"),
            ("zip", "''"),
            ("country", "''"),
            ("phone", "''"),
            ("email", "''"),
            ("notes", "''"),
        ],
        "ORDER BY id",
    )? {
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok(LegacyCustomer {
                id: r.get(0)?,
                name: r.get(1)?,
                street: r.get(2)?,
                city: r.get(3)?,
                zip: r.get(4)?,
                country: r.get(5)?,
                phone: r.get(6)?,
                email: r.get(7)?,
                notes: r.get(8)?,
            })
        })?;
        data.customers = rows.collect::<rusqlite::Result<_>>()?;
    }
    if let Some(sql) = select(
        conn,
        "products",
        &[
            ("id", "0"),
            ("seller_id", "0"),
            ("name", "''"),
            ("description", "''"),
            ("type", "'product'"),
            ("unit", "'Stk'"),
            ("price_net", "0"),
            ("tax_rate", "19"),
            ("stock", "0"),
            ("active", "1"),
        ],
        "ORDER BY id",
    )? {
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok(LegacyProduct {
                id: r.get(0)?,
                seller_id: r.get(1)?,
                name: r.get(2)?,
                description: r.get(3)?,
                kind: r.get(4)?,
                unit: r.get(5)?,
                price_net: r.get(6)?,
                tax_rate: r.get(7)?,
                stock: r.get(8)?,
                active: r.get::<_, i64>(9)? != 0,
            })
        })?;
        data.products = rows.collect::<rusqlite::Result<_>>()?;
    }
    if let Some(sql) = select(
        conn,
        "invoices",
        &[
            ("id", "0"),
            ("seller_id", "0"),
            ("customer_id", "0"),
            ("invoice_number", "''"),
            ("date", "''"),
            ("due_date", "''"),
            ("status", "'draft'"),
            ("notes", "''"),
            ("payment_terms", "''"),
            ("total_net", "0"),
            ("total_tax", "0"),
            ("total_gross", "0"),
            ("shipping_net", "0"),
            ("shipping_tax_rate", "19"),
            ("already_paid", "0"),
        ],
        "ORDER BY id",
    )? {
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok(LegacyInvoice {
                id: r.get(0)?,
                seller_id: r.get(1)?,
                customer_id: r.get(2)?,
                invoice_number: r.get(3)?,
                date: r.get(4)?,
                due_date: r.get(5)?,
                status: r.get(6)?,
                notes: r.get(7)?,
                payment_terms: r.get(8)?,
                total_net: r.get(9)?,
                total_tax: r.get(10)?,
                total_gross: r.get(11)?,
                shipping_net: r.get(12)?,
                shipping_tax_rate: r.get(13)?,
                already_paid: r.get::<_, i64>(14)? != 0,
                items: Vec::new(),
            })
        })?;
        data.invoices = rows.collect::<rusqlite::Result<_>>()?;
    }
    if let Some(sql) = select(
        conn,
        "invoice_items",
        &[
            ("invoice_id", "0"),
            ("product_id", "NULL"),
            ("position", "1"),
            ("description", "''"),
            ("quantity", "1"),
            ("unit", "''"),
            ("price_net", "0"),
            ("tax_rate", "19"),
        ],
        "ORDER BY invoice_id, position",
    )? {
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                LegacyItem {
                    product_id: r.get(1)?,
                    position: r.get(2)?,
                    description: r.get(3)?,
                    quantity: r.get(4)?,
                    unit: r.get(5)?,
                    price_net: r.get(6)?,
                    tax_rate: r.get(7)?,
                },
            ))
        })?;
        for row in rows {
            let (invoice_id, item) = row?;
            if let Some(invoice) = data.invoices.iter_mut().find(|i| i.id == invoice_id) {
                invoice.items.push(item);
            }
        }
    }
    if let Some(sql) = select(
        conn,
        "payments",
        &[
            ("id", "0"),
            ("invoice_id", "0"),
            ("amount", "0"),
            ("date", "''"),
            ("method", "''"),
            ("notes", "''"),
        ],
        "ORDER BY id",
    )? {
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok(LegacyPayment {
                id: r.get(0)?,
                invoice_id: r.get(1)?,
                amount: r.get(2)?,
                date: r.get(3)?,
                method: r.get(4)?,
                notes: r.get(5)?,
            })
        })?;
        data.payments = rows.collect::<rusqlite::Result<_>>()?;
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issue::sha256_hex;
    use crate::legacy::testdata::{LOGO_DATA_URL, early_db, full_db};

    #[test]
    fn finds_both_file_names() {
        let config = tempfile::tempdir().unwrap();
        assert!(find_legacy_databases(config.path()).is_empty());
        let dir = config.path().join("de.stelle.vibebill");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("vibebill.db"), b"x").unwrap();
        std::fs::write(dir.join("rechnung.db"), b"x").unwrap();
        let found = find_legacy_databases(config.path());
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn reads_full_v3_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vibebill.db");
        full_db(&path);
        let data = read_legacy(&path, dir.path()).unwrap();
        assert_eq!(
            (
                data.sellers.len(),
                data.customers.len(),
                data.products.len(),
                data.invoices.len(),
                data.payments.len()
            ),
            (2, 2, 2, 6, 1)
        );
        let second = data.invoices.iter().find(|i| i.id == 2).unwrap();
        assert_eq!(second.shipping_net, 4.9);
        assert_eq!(second.items.len(), 1);
        let band = data.sellers.iter().find(|s| s.id == 1).unwrap();
        assert_eq!(band.logo.as_ref().map(|l| l.1.as_str()), Some("image/png"));
        assert_eq!(band.pdf_template, "modern");
    }

    #[test]
    fn reads_early_database_without_js_columns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vibebill.db");
        early_db(&path);
        let data = read_legacy(&path, dir.path()).unwrap();
        let invoice = &data.invoices[0];
        assert_eq!(
            (
                invoice.shipping_net,
                invoice.shipping_tax_rate,
                invoice.already_paid
            ),
            (0.0, 19.0, false)
        );
        let seller = &data.sellers[0];
        assert_eq!(
            (
                seller.currency.as_str(),
                seller.color.as_str(),
                seller.pdf_template.as_str()
            ),
            ("EUR", "#3b82f6", "classic")
        );
        assert!(data.payments.is_empty());
        assert_eq!(invoice.items.len(), 1);
    }

    #[test]
    fn source_file_is_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("vibebill.db");
        full_db(&source);
        let before = sha256_hex(&std::fs::read(&source).unwrap());
        let scratch = dir.path().join("scratch");
        std::fs::create_dir_all(&scratch).unwrap();
        read_legacy(&source, &scratch).unwrap();
        assert_eq!(sha256_hex(&std::fs::read(&source).unwrap()), before);
        assert!(!dir.path().join("vibebill.db-wal").exists());
    }

    #[test]
    fn decodes_logo_data_url() {
        let (bytes, mime) = decode_data_url(LOGO_DATA_URL).unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(&bytes[1..4], b"PNG");
        assert!(decode_data_url("kein bild").is_none());
        assert!(decode_data_url("").is_none());
    }
}
