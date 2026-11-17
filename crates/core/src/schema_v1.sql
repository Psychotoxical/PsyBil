-- PsyBil schema version 1.
-- IDs are UUID text, dates YYYY-MM-DD, timestamps RFC 3339 UTC,
-- money INTEGER cents, quantities and prices decimal TEXT.

CREATE TABLE businesses (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    street TEXT NOT NULL DEFAULT '',
    zip TEXT NOT NULL DEFAULT '',
    city TEXT NOT NULL DEFAULT '',
    country TEXT NOT NULL DEFAULT 'DE',
    tax_number TEXT NOT NULL DEFAULT '',
    vat_id TEXT NOT NULL DEFAULT '',
    small_business INTEGER NOT NULL DEFAULT 0,
    small_business_since TEXT,
    currency TEXT NOT NULL DEFAULT 'EUR',
    number_template TEXT NOT NULL DEFAULT '{PREFIX}-{YYYY}-{NNNN}',
    storno_prefix TEXT NOT NULL DEFAULT 'ST',
    tax_rates TEXT NOT NULL DEFAULT '["19","7"]',
    archived INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE brands (
    id TEXT PRIMARY KEY,
    business_id TEXT NOT NULL REFERENCES businesses(id),
    name TEXT NOT NULL,
    logo BLOB,
    logo_mime TEXT,
    color TEXT NOT NULL DEFAULT '#8839ef',
    phone TEXT NOT NULL DEFAULT '',
    email TEXT NOT NULL DEFAULT '',
    website TEXT NOT NULL DEFAULT '',
    bank_name TEXT NOT NULL DEFAULT '',
    iban TEXT NOT NULL DEFAULT '',
    bic TEXT NOT NULL DEFAULT '',
    pdf_template TEXT NOT NULL DEFAULT 'classic',
    invoice_prefix TEXT NOT NULL DEFAULT 'RE',
    payment_days INTEGER NOT NULL DEFAULT 14,
    intro_text TEXT NOT NULL DEFAULT '',
    outro_text TEXT NOT NULL DEFAULT '',
    price_mode TEXT NOT NULL DEFAULT 'net' CHECK (price_mode IN ('net', 'gross')),
    archived INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX brands_business ON brands(business_id);

CREATE TABLE customers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    street TEXT NOT NULL DEFAULT '',
    zip TEXT NOT NULL DEFAULT '',
    city TEXT NOT NULL DEFAULT '',
    country TEXT NOT NULL DEFAULT 'DE',
    kind TEXT NOT NULL DEFAULT 'unknown' CHECK (kind IN ('private', 'business', 'unknown')),
    vat_id TEXT NOT NULL DEFAULT '',
    email TEXT NOT NULL DEFAULT '',
    phone TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    language TEXT NOT NULL DEFAULT 'de',
    archived INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE products (
    id TEXT PRIMARY KEY,
    brand_id TEXT NOT NULL REFERENCES brands(id),
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL CHECK (kind IN ('goods', 'service')),
    unit TEXT NOT NULL DEFAULT 'Stk',
    price TEXT NOT NULL,
    price_mode TEXT NOT NULL CHECK (price_mode IN ('net', 'gross')),
    tax TEXT NOT NULL,
    stock TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX products_brand ON products(brand_id);

CREATE TABLE invoices (
    id TEXT PRIMARY KEY,
    business_id TEXT NOT NULL REFERENCES businesses(id),
    brand_id TEXT NOT NULL REFERENCES brands(id),
    customer_id TEXT REFERENCES customers(id),
    kind TEXT NOT NULL DEFAULT 'invoice' CHECK (kind IN ('invoice', 'storno')),
    state TEXT NOT NULL DEFAULT 'draft' CHECK (state IN ('draft', 'issued')),
    number TEXT,
    invoice_date TEXT NOT NULL,
    service_date TEXT NOT NULL,
    payment_days INTEGER NOT NULL,
    due_date TEXT NOT NULL,
    price_mode TEXT NOT NULL CHECK (price_mode IN ('net', 'gross')),
    currency TEXT NOT NULL,
    language TEXT NOT NULL,
    intro_text TEXT NOT NULL DEFAULT '',
    outro_text TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    net_cents INTEGER,
    tax_cents INTEGER,
    gross_cents INTEGER,
    cancels_id TEXT REFERENCES invoices(id),
    cancelled_by_id TEXT REFERENCES invoices(id),
    replaces_id TEXT REFERENCES invoices(id),
    imported INTEGER NOT NULL DEFAULT 0,
    needs_review INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (business_id, number)
);
CREATE INDEX invoices_business ON invoices(business_id);
CREATE INDEX invoices_customer ON invoices(customer_id);

CREATE TABLE invoice_lines (
    id TEXT PRIMARY KEY,
    invoice_id TEXT NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('item', 'free', 'shipping', 'discount')),
    product_id TEXT REFERENCES products(id) ON DELETE SET NULL,
    description TEXT NOT NULL DEFAULT '',
    quantity TEXT NOT NULL,
    unit TEXT NOT NULL DEFAULT '',
    unit_price TEXT NOT NULL,
    tax TEXT NOT NULL
);
CREATE INDEX invoice_lines_invoice ON invoice_lines(invoice_id);
CREATE INDEX invoice_lines_product ON invoice_lines(product_id);

CREATE TABLE payments (
    id TEXT PRIMARY KEY,
    invoice_id TEXT NOT NULL REFERENCES invoices(id),
    amount_cents INTEGER NOT NULL,
    date TEXT NOT NULL,
    method TEXT NOT NULL DEFAULT 'other'
        CHECK (method IN ('transfer', 'cash', 'paypal', 'other', 'legacy')),
    note TEXT NOT NULL DEFAULT '',
    transfer_of_id TEXT REFERENCES payments(id),
    transferred_to_id TEXT REFERENCES invoices(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX payments_invoice ON payments(invoice_id);

CREATE TABLE number_counters (
    business_id TEXT NOT NULL REFERENCES businesses(id),
    year INTEGER NOT NULL,
    next INTEGER NOT NULL,
    PRIMARY KEY (business_id, year)
);

CREATE TABLE invoice_snapshots (
    invoice_id TEXT PRIMARY KEY REFERENCES invoices(id),
    snapshot TEXT NOT NULL,
    pdf BLOB,
    pdf_sha256 TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE stock_moves (
    id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL REFERENCES products(id),
    invoice_id TEXT NOT NULL REFERENCES invoices(id),
    delta TEXT NOT NULL,
    at TEXT NOT NULL
);

CREATE TABLE audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at TEXT NOT NULL,
    entity TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    data TEXT NOT NULL
);

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Issued invoices are immutable (GoBD). Only the cancellation link, the
-- review flag and updated_at may change.
CREATE TRIGGER invoices_issued_no_delete BEFORE DELETE ON invoices
WHEN OLD.state = 'issued'
BEGIN SELECT RAISE(ABORT, 'issued invoice cannot be deleted'); END;

CREATE TRIGGER invoices_issued_no_update BEFORE UPDATE ON invoices
WHEN OLD.state = 'issued' AND (
    NEW.state IS NOT OLD.state OR NEW.number IS NOT OLD.number
    OR NEW.business_id IS NOT OLD.business_id OR NEW.brand_id IS NOT OLD.brand_id
    OR NEW.customer_id IS NOT OLD.customer_id OR NEW.kind IS NOT OLD.kind
    OR NEW.invoice_date IS NOT OLD.invoice_date OR NEW.service_date IS NOT OLD.service_date
    OR NEW.payment_days IS NOT OLD.payment_days OR NEW.due_date IS NOT OLD.due_date
    OR NEW.price_mode IS NOT OLD.price_mode OR NEW.currency IS NOT OLD.currency
    OR NEW.language IS NOT OLD.language OR NEW.intro_text IS NOT OLD.intro_text
    OR NEW.outro_text IS NOT OLD.outro_text OR NEW.notes IS NOT OLD.notes
    OR NEW.net_cents IS NOT OLD.net_cents OR NEW.tax_cents IS NOT OLD.tax_cents
    OR NEW.gross_cents IS NOT OLD.gross_cents OR NEW.cancels_id IS NOT OLD.cancels_id
    OR NEW.replaces_id IS NOT OLD.replaces_id OR NEW.imported IS NOT OLD.imported
    OR (OLD.cancelled_by_id IS NOT NULL AND NEW.cancelled_by_id IS NOT OLD.cancelled_by_id)
)
BEGIN SELECT RAISE(ABORT, 'issued invoice is immutable'); END;

CREATE TRIGGER lines_issued_no_insert BEFORE INSERT ON invoice_lines
WHEN (SELECT state FROM invoices WHERE id = NEW.invoice_id) = 'issued'
BEGIN SELECT RAISE(ABORT, 'lines of an issued invoice are immutable'); END;

CREATE TRIGGER lines_issued_no_update BEFORE UPDATE ON invoice_lines
WHEN (SELECT state FROM invoices WHERE id = OLD.invoice_id) = 'issued'
BEGIN SELECT RAISE(ABORT, 'lines of an issued invoice are immutable'); END;

CREATE TRIGGER lines_issued_no_delete BEFORE DELETE ON invoice_lines
WHEN (SELECT state FROM invoices WHERE id = OLD.invoice_id) = 'issued'
BEGIN SELECT RAISE(ABORT, 'lines of an issued invoice are immutable'); END;

CREATE TRIGGER audit_no_update BEFORE UPDATE ON audit_log
BEGIN SELECT RAISE(ABORT, 'audit log is append-only'); END;

CREATE TRIGGER audit_no_delete BEFORE DELETE ON audit_log
BEGIN SELECT RAISE(ABORT, 'audit log is append-only'); END;

-- Snapshots never change, except that the PDF may be added once.
CREATE TRIGGER snapshots_no_update BEFORE UPDATE ON invoice_snapshots
WHEN NEW.invoice_id IS NOT OLD.invoice_id OR NEW.snapshot IS NOT OLD.snapshot
    OR NEW.created_at IS NOT OLD.created_at
    OR (OLD.pdf IS NOT NULL AND NEW.pdf IS NOT OLD.pdf)
    OR (OLD.pdf_sha256 IS NOT NULL AND NEW.pdf_sha256 IS NOT OLD.pdf_sha256)
BEGIN SELECT RAISE(ABORT, 'snapshot is immutable'); END;

CREATE TRIGGER snapshots_no_delete BEFORE DELETE ON invoice_snapshots
BEGIN SELECT RAISE(ABORT, 'snapshot is immutable'); END;
