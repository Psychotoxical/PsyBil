use thiserror::Error;

use crate::lockfile::LockInfo;
use crate::money::CalcError;
use crate::validate::Issue;

/// Errors returned by the core.
#[derive(Debug, Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("calculation error: {0}")]
    Calc(#[from] CalcError),
    #[error("invoice cannot be issued: {} problem(s)", .0.len())]
    Validation(Vec<Issue>),
    #[error("not found")]
    NotFound,
    #[error("invoice is not a draft")]
    NotDraft,
    #[error("invoice is already issued")]
    AlreadyIssued,
    #[error("invoice is not issued")]
    NotIssued,
    #[error("invoice cannot be cancelled")]
    NotCancellable,
    #[error("number template needs {{N}} or {{NNNN}}")]
    InvalidNumberTemplate,
    #[error("payment was moved to a replacement invoice")]
    PaymentTransferred,
    #[error("database is from a newer PsyBil (schema {found}, supported {supported})")]
    NewerSchema { found: i32, supported: i32 },
    #[error("database has an older schema ({found}) and must be opened for writing first")]
    OlderSchema { found: i32 },
    #[error("file error: {0}")]
    Io(String),
    #[error("the data is being edited on {}", .0.device_name)]
    LockHeld(LockInfo),
    #[error("the lock of {} looks abandoned", .0.device_name)]
    LockStale(LockInfo),
    #[error("the lock was lost")]
    LockLost,
    #[error("the data is open read-only")]
    ReadOnly,
    #[error("a PsyBil database already exists there")]
    TargetNotEmpty,
    #[error("import failed: {0}")]
    Import(String),
    #[error("merge failed: {0}")]
    Merge(String),
    #[error("PDF failed: {0}")]
    Pdf(String),
    #[error("in use by an issued invoice")]
    InUseByIssuedInvoice,
    #[error("counter too low, number {existing} already exists")]
    CounterTooLow { existing: u32 },
}

pub type Result<T> = std::result::Result<T, Error>;
