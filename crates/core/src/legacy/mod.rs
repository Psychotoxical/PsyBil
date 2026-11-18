//! Taking over data from VibeBill, the predecessor of PsyBil. The old
//! database is only ever read through a copy, never changed.

pub mod apply;
pub mod plan;
pub mod read;

/// Synthetic old databases, for tests here and in the app.
#[cfg(any(test, feature = "testdata"))]
pub mod testdata;

pub use apply::*;
pub use plan::*;
pub use read::*;
