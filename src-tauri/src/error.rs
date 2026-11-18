//! Errors as the window gets them: a stable code it can translate, the
//! core's message as a fallback, and the missing items when issuing fails.

use psybil_core::Error;
use psybil_core::money::CalcError;
use psybil_core::validate::Issue;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CmdError {
    pub code: String,
    pub message: String,
    pub issues: Vec<Issue>,
}

impl CmdError {
    pub fn new(code: &str, message: impl Into<String>) -> CmdError {
        CmdError {
            code: code.into(),
            message: message.into(),
            issues: Vec::new(),
        }
    }
}

/// The name of the error variant in snake_case.
pub fn code_of(error: &Error) -> &'static str {
    match error {
        Error::Db(_) => "db",
        Error::Json(_) => "json",
        Error::Calc(calc) => match calc {
            CalcError::Empty | CalcError::Invalid => "invalid_number",
            CalcError::TooManyDecimals => "too_many_decimals",
            CalcError::Overflow => "amount_too_large",
            CalcError::InvalidRate => "invalid_rate",
            CalcError::NegativeTotal => "negative_total",
        },
        Error::Validation(_) => "validation",
        Error::NotFound => "not_found",
        Error::NotDraft => "not_draft",
        Error::AlreadyIssued => "already_issued",
        Error::NotIssued => "not_issued",
        Error::NotCancellable => "not_cancellable",
        Error::InvalidNumberTemplate => "invalid_number_template",
        Error::PaymentTransferred => "payment_transferred",
        Error::NewerSchema { .. } => "newer_schema",
        Error::OlderSchema { .. } => "older_schema",
        Error::Io(_) => "io",
        Error::LockHeld(_) => "lock_held",
        Error::LockStale(_) => "lock_stale",
        Error::LockLost => "lock_lost",
        Error::ReadOnly => "read_only",
        Error::TargetNotEmpty => "target_not_empty",
        Error::Import(_) => "import",
        Error::Merge(_) => "merge",
        Error::BackupDamaged(_) => "backup_damaged",
        Error::NotABackup => "not_a_backup",
        Error::SharedFileMissing => "shared_file_missing",
        Error::DataNotOpen => "no_data",
        Error::Pdf(_) => "pdf",
        Error::InUseByIssuedInvoice => "in_use_by_issued_invoice",
        Error::CounterTooLow { .. } => "counter_too_low",
    }
}

impl From<Error> for CmdError {
    fn from(error: Error) -> CmdError {
        let issues = match &error {
            Error::Validation(issues) => issues.clone(),
            _ => Vec::new(),
        };
        CmdError {
            code: code_of(&error).into(),
            message: error.to_string(),
            issues,
        }
    }
}

pub type CmdResult<T> = Result<T, CmdError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_snake_case_variants() {
        assert_eq!(
            CmdError::from(Error::Validation(Vec::new())).code,
            "validation"
        );
        assert_eq!(
            code_of(&Error::InUseByIssuedInvoice),
            "in_use_by_issued_invoice"
        );
        assert_eq!(code_of(&Error::NotFound), "not_found");
        assert_eq!(code_of(&Error::PaymentTransferred), "payment_transferred");
        assert_eq!(code_of(&Error::Pdf("x".into())), "pdf");
        // Calculation errors say which kind, so the window can name the problem.
        assert_eq!(
            code_of(&Error::Calc(CalcError::TooManyDecimals)),
            "too_many_decimals"
        );
        assert_eq!(
            code_of(&Error::Calc(CalcError::Overflow)),
            "amount_too_large"
        );
        assert_eq!(code_of(&Error::Calc(CalcError::Invalid)), "invalid_number");
        assert_eq!(code_of(&Error::Calc(CalcError::Empty)), "invalid_number");
        assert_eq!(
            code_of(&Error::Calc(CalcError::InvalidRate)),
            "invalid_rate"
        );
        assert_eq!(
            code_of(&Error::Calc(CalcError::NegativeTotal)),
            "negative_total"
        );
        assert_eq!(
            code_of(&Error::NewerSchema {
                found: 3,
                supported: 2
            }),
            "newer_schema"
        );
    }
}
