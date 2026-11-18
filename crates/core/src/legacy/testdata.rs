//! Synthetic VibeBill databases for tests. Neutral placeholder data only.

use std::path::Path;

use rusqlite::Connection;

/// VibeBill schema as of 1.0.30: Tauri migrations 1 to 3 plus the columns
/// its JavaScript added at runtime.
const FULL_SCHEMA: &str = "
CREATE TABLE sellers (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, street TEXT DEFAULT '',
  city TEXT DEFAULT '', zip TEXT DEFAULT '', first_name TEXT DEFAULT '', last_name TEXT DEFAULT '',
  default_payment_terms TEXT DEFAULT '', default_tax_rate REAL DEFAULT NULL, currency TEXT DEFAULT '',
  default_note TEXT DEFAULT '', country TEXT DEFAULT 'Deutschland', phone TEXT DEFAULT '', email TEXT DEFAULT '',
  website TEXT DEFAULT '', tax_id TEXT DEFAULT '', vat_id TEXT DEFAULT '', bank_name TEXT DEFAULT '',
  bank_iban TEXT DEFAULT '', bank_bic TEXT DEFAULT '', logo_data TEXT DEFAULT '', invoice_prefix TEXT DEFAULT 'RE',
  next_invoice_number INTEGER DEFAULT 1, created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  pdf_template TEXT DEFAULT 'classic', color TEXT DEFAULT '#3b82f6');
CREATE TABLE customers (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, street TEXT DEFAULT '',
  city TEXT DEFAULT '', zip TEXT DEFAULT '', country TEXT DEFAULT 'Deutschland', phone TEXT DEFAULT '',
  email TEXT DEFAULT '', notes TEXT DEFAULT '', created_at DATETIME DEFAULT CURRENT_TIMESTAMP);
CREATE TABLE products (id INTEGER PRIMARY KEY AUTOINCREMENT, seller_id INTEGER NOT NULL, name TEXT NOT NULL,
  description TEXT DEFAULT '', type TEXT NOT NULL DEFAULT 'product', unit TEXT DEFAULT 'Stk',
  price_net REAL NOT NULL DEFAULT 0, tax_rate REAL NOT NULL DEFAULT 19.0, stock INTEGER DEFAULT 0,
  active INTEGER DEFAULT 1, created_at DATETIME DEFAULT CURRENT_TIMESTAMP);
CREATE TABLE invoices (id INTEGER PRIMARY KEY AUTOINCREMENT, seller_id INTEGER NOT NULL, customer_id INTEGER NOT NULL,
  invoice_number TEXT NOT NULL, date TEXT NOT NULL, due_date TEXT DEFAULT '', status TEXT DEFAULT 'draft',
  notes TEXT DEFAULT '', payment_terms TEXT DEFAULT '14 Tage netto', total_net REAL DEFAULT 0,
  total_tax REAL DEFAULT 0, total_gross REAL DEFAULT 0, created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  shipping_net REAL DEFAULT 0, shipping_tax_rate REAL DEFAULT 19, already_paid INTEGER DEFAULT 0);
CREATE TABLE invoice_items (id INTEGER PRIMARY KEY AUTOINCREMENT, invoice_id INTEGER NOT NULL, product_id INTEGER,
  position INTEGER NOT NULL DEFAULT 1, description TEXT NOT NULL DEFAULT '', quantity REAL NOT NULL DEFAULT 1,
  unit TEXT DEFAULT 'Stk', price_net REAL NOT NULL DEFAULT 0, tax_rate REAL NOT NULL DEFAULT 19.0,
  total_net REAL NOT NULL DEFAULT 0, total_tax REAL NOT NULL DEFAULT 0, total_gross REAL NOT NULL DEFAULT 0);
CREATE TABLE payments (id INTEGER PRIMARY KEY AUTOINCREMENT, invoice_id INTEGER NOT NULL, amount REAL NOT NULL DEFAULT 0,
  date TEXT NOT NULL, method TEXT DEFAULT '', notes TEXT DEFAULT '', created_at DATETIME DEFAULT CURRENT_TIMESTAMP);
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT DEFAULT '');
";

/// A 1×1 transparent PNG as data URL.
pub const LOGO_DATA_URL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/// A full VibeBill 1.0.30 database with the cases the import must handle.
pub fn full_db(path: &Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(FULL_SCHEMA).unwrap();
    conn.execute(
        "INSERT INTO sellers (id, name, street, city, zip, tax_id, bank_name, bank_iban, invoice_prefix,
            next_invoice_number, default_payment_terms, default_note, logo_data, color, pdf_template)
         VALUES (1, 'Die Band', 'Musterstraße 1', 'Musterstadt', '12345', '12/345/67890', 'Bank Beispiel',
            'DE00000000000000000000', 'RE', 6, '14 Tage netto', 'Danke für die Zusammenarbeit!', ?1, '#7c3aed', 'modern')",
        [LOGO_DATA_URL],
    )
    .unwrap();
    conn.execute_batch(
        "INSERT INTO sellers (id, name, street, city, zip, tax_id, invoice_prefix, next_invoice_number,
            default_payment_terms)
         VALUES (2, 'Studio Beispiel', 'Lindenweg 4', 'Musterstadt', '12345', '98/765/43210', 'SB', 2, 'Sofort fällig');
         INSERT INTO customers (id, name, street, city, zip, country) VALUES
           (1, 'Club Beispiel', 'Am Markt 5', 'Beispielstadt', '54321', 'Deutschland'),
           (2, 'Kunde A', 'Hauptstraße 2', 'Beispielstadt', '54321', '');
         INSERT INTO products (id, seller_id, name, type, unit, price_net, tax_rate, stock) VALUES
           (1, 1, 'Shirt', 'product', 'Stk', 16.81, 19, 12),
           (2, 2, 'Studiozeit', 'service', 'Std', 45, 19, 0);
         INSERT INTO invoices (id, seller_id, customer_id, invoice_number, date, due_date, status,
            total_net, total_tax, total_gross, shipping_net, shipping_tax_rate, already_paid) VALUES
           (1, 1, 1, 'RE-2026-0001', '2026-03-10', '2026-03-24', 'paid', 340, 0, 340, 0, 19, 1),
           (2, 1, 2, 'RE-2026-0002', '2026-04-02', '2026-04-16', 'sent', 38.52, 6.39, 44.91, 4.9, 19, 0),
           (3, 1, 1, 'RE-2026-0003', '2026-05-01', '2026-05-15', 'draft', 450, 0, 450, 0, 19, 0),
           (4, 1, 2, 'RE-2026-0004', '2026-05-05', '2026-05-19', 'cancelled', 12, 2.28, 14.28, 0, 19, 0),
           (5, 1, 1, 'RE-2026-0004', '2026-05-06', '2026-05-20', 'sent', 10, 1.9, 12, 0, 19, 0),
           (6, 2, 1, 'SB-2026-0001', '2026-06-01', '2026-06-01', 'overdue', 180, 34.2, 214.2, 0, 19, 0);
         INSERT INTO invoice_items (invoice_id, product_id, position, description, quantity, unit, price_net,
            tax_rate, total_net, total_tax, total_gross) VALUES
           (1, NULL, 1, 'Gage Auftritt', 1, 'Pausch.', 300, 0, 300, 0, 300),
           (1, NULL, 2, 'Fahrtkosten', 1, 'Pausch.', 40, 0, 40, 0, 40),
           (2, 1, 1, 'Shirt', 2, 'Stk', 16.81, 19, 33.62, 6.39, 40.01),
           (3, NULL, 1, 'Gage Auftritt', 1, 'Pausch.', 450, 0, 450, 0, 450),
           (4, NULL, 1, 'CD', 1, 'Stk', 12, 19, 12, 2.28, 14.28),
           (5, NULL, 1, 'Aufkleber', 1, 'Stk', 10, 19, 10, 1.9, 11.9),
           (6, 2, 1, 'Studiozeit', 4, 'Std', 45, 19, 180, 34.2, 214.2);
         INSERT INTO payments (invoice_id, amount, date, method) VALUES (2, 20, '2026-04-10', 'bank');",
    )
    .unwrap();
}

/// An early VibeBill database: no payments table, none of the columns that
/// later versions added.
pub fn early_db(path: &Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE sellers (id INTEGER PRIMARY KEY, name TEXT NOT NULL, street TEXT, city TEXT, zip TEXT,
            country TEXT, phone TEXT, email TEXT, website TEXT, tax_id TEXT, vat_id TEXT, bank_name TEXT,
            bank_iban TEXT, bank_bic TEXT, logo_data TEXT, invoice_prefix TEXT, next_invoice_number INTEGER);
         CREATE TABLE customers (id INTEGER PRIMARY KEY, name TEXT NOT NULL, street TEXT, city TEXT, zip TEXT,
            country TEXT, phone TEXT, email TEXT, notes TEXT);
         CREATE TABLE products (id INTEGER PRIMARY KEY, seller_id INTEGER, name TEXT, description TEXT, type TEXT,
            unit TEXT, price_net REAL, tax_rate REAL, stock INTEGER, active INTEGER);
         CREATE TABLE invoices (id INTEGER PRIMARY KEY, seller_id INTEGER, customer_id INTEGER, invoice_number TEXT,
            date TEXT, due_date TEXT, status TEXT, notes TEXT, payment_terms TEXT, total_net REAL, total_tax REAL,
            total_gross REAL);
         CREATE TABLE invoice_items (id INTEGER PRIMARY KEY, invoice_id INTEGER, product_id INTEGER, position INTEGER,
            description TEXT, quantity REAL, unit TEXT, price_net REAL, tax_rate REAL, total_net REAL,
            total_tax REAL, total_gross REAL);
         INSERT INTO sellers (id, name, street, city, zip, tax_id, invoice_prefix, next_invoice_number)
            VALUES (1, 'Die Band', 'Musterstraße 1', 'Musterstadt', '12345', '12/345/67890', 'RE', 2);
         INSERT INTO customers (id, name, street, city, zip) VALUES (1, 'Club Beispiel', 'Am Markt 5', 'Beispielstadt', '54321');
         INSERT INTO invoices (id, seller_id, customer_id, invoice_number, date, due_date, status, total_net,
            total_tax, total_gross) VALUES (1, 1, 1, 'RE-2026-0001', '2026-02-20', '2026-03-06', 'sent', 200, 0, 200);
         INSERT INTO invoice_items (invoice_id, position, description, quantity, unit, price_net, tax_rate,
            total_net, total_tax, total_gross) VALUES (1, 1, 'Gage Auftritt', 1, 'Pausch.', 200, 0, 200, 0, 200);",
    )
    .unwrap();
}

#[cfg(test)]
mod tests {
    /// Writes a synthetic VibeBill database to `PSYBIL_SAMPLES/vibebill.db`,
    /// for trying the import assistant by hand.
    #[test]
    #[ignore]
    fn write_sample_legacy_db() {
        let dir = std::path::PathBuf::from(std::env::var("PSYBIL_SAMPLES").unwrap());
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("vibebill.db");
        let _ = std::fs::remove_file(&path);
        super::full_db(&path);
    }
}
