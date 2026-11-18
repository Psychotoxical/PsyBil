//! PsyBil core: all business rules for invoices, taxes, numbering and storage.

pub mod audit;
pub mod calc;
pub mod dates;
pub mod db;
pub mod demo;
pub mod error;
pub mod invoice;
pub mod issue;
pub mod legacy;
pub mod listing;
pub mod lockfile;
pub mod masterdata;
pub mod merge;
pub mod money;
pub mod numbering;
pub mod payments;
pub mod pdf;
pub mod settings;
pub mod storno;
pub mod tax;
pub mod tombstone;
pub mod validate;
pub mod workspace;

#[cfg(test)]
pub(crate) mod testutil;

pub use error::{Error, Result};
pub use rusqlite;
pub use rust_decimal;
