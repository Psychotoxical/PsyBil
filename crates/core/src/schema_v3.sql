-- PsyBil schema version 3: the overview and originals of imported invoices.

-- Revenue that does not run through PsyBil, one sum per business and year. The id is
-- derived from business and year, so two devices write the same row and a merge keeps
-- the newer one.
CREATE TABLE outside_revenue (
    id TEXT PRIMARY KEY,
    business_id TEXT NOT NULL REFERENCES businesses(id),
    year INTEGER NOT NULL,
    amount_cents INTEGER NOT NULL CHECK (amount_cents >= 0),
    note TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (business_id, year)
);

-- The PDF that was sent at the time, attached to an imported invoice. Only added to.
CREATE TABLE invoice_attachments (
    id TEXT PRIMARY KEY,
    invoice_id TEXT NOT NULL REFERENCES invoices(id),
    kind TEXT NOT NULL CHECK (kind IN ('original_pdf')),
    file_name TEXT NOT NULL,
    bytes BLOB NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX invoice_attachments_invoice ON invoice_attachments(invoice_id);
CREATE TRIGGER invoice_attachments_no_update BEFORE UPDATE ON invoice_attachments
BEGIN SELECT RAISE(ABORT, 'attachments are append-only'); END;
CREATE TRIGGER invoice_attachments_no_delete BEFORE DELETE ON invoice_attachments
BEGIN SELECT RAISE(ABORT, 'attachments are append-only'); END;
