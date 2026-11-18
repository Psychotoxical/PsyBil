-- PsyBil schema version 2: merging across devices and legacy import.

-- Audit rows get a stable id so two copies can be merged without duplicates.
ALTER TABLE audit_log ADD COLUMN uid TEXT;
DROP TRIGGER audit_no_update;
UPDATE audit_log SET uid = lower(hex(randomblob(16))) WHERE uid IS NULL;
CREATE TRIGGER audit_no_update BEFORE UPDATE ON audit_log
BEGIN SELECT RAISE(ABORT, 'audit log is append-only'); END;
CREATE UNIQUE INDEX audit_log_uid ON audit_log(uid);

-- Deletions are remembered, so a merge does not bring deleted rows back.
CREATE TABLE tombstones (
    table_name TEXT NOT NULL,
    id TEXT NOT NULL,
    at TEXT NOT NULL,
    PRIMARY KEY (table_name, id)
);

-- Documents a merge or import could not place, for example a duplicate number.
CREATE TABLE merge_quarantine (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    reason TEXT NOT NULL,
    payload TEXT NOT NULL,
    at TEXT NOT NULL
);

-- Invoices taken over from VibeBill as cancelled, without a cancellation document.
ALTER TABLE invoices ADD COLUMN legacy_cancelled INTEGER NOT NULL DEFAULT 0;

DROP TRIGGER invoices_issued_no_update;
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
    OR NEW.legacy_cancelled IS NOT OLD.legacy_cancelled
    OR (OLD.cancelled_by_id IS NOT NULL AND NEW.cancelled_by_id IS NOT OLD.cancelled_by_id)
)
BEGIN SELECT RAISE(ABORT, 'issued invoice is immutable'); END;
