use thiserror::Error;

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
    #[error("in use by an issued invoice")]
    InUseByIssuedInvoice,
    #[error("counter too low, number {existing} already exists")]
    CounterTooLow { existing: u32 },
}

pub type Result<T> = std::result::Result<T, Error>;
