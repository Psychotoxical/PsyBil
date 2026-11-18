//! Dates. Invoices use calendar dates only; "today" always comes from a
//! `Clock`, so there is no UTC round trip and tests can fix the date.

use chrono::{DateTime, Days, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Source of the current date and time.
pub trait Clock {
    /// Today's date in the local time zone.
    fn today(&self) -> NaiveDate;
    /// Current instant, for timestamps.
    fn now_utc(&self) -> DateTime<Utc>;
}

/// The system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn today(&self) -> NaiveDate {
        Local::now().date_naive()
    }
    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A clock fixed to one day, for tests.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    pub today: NaiveDate,
}

impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        self.today
    }
    fn now_utc(&self) -> DateTime<Utc> {
        self.today
            .and_hms_opt(0, 0, 0)
            .expect("midnight exists")
            .and_utc()
    }
}

/// RFC 3339 UTC timestamp for `created_at`, `updated_at` and the audit log.
pub fn timestamp(clock: &dyn Clock) -> String {
    clock
        .now_utc()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Due date: invoice date plus the payment term in days.
pub fn due_date(invoice_date: NaiveDate, payment_days: u32) -> NaiveDate {
    invoice_date
        .checked_add_days(Days::new(u64::from(payment_days)))
        .unwrap_or(NaiveDate::MAX)
}

/// When the goods were delivered or the service was performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum ServiceDate {
    /// Printed as "service date equals invoice date".
    SameAsInvoice,
    Day {
        date: NaiveDate,
    },
    Period {
        from: NaiveDate,
        to: NaiveDate,
    },
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn due_date_across_dst_change() {
        assert_eq!(due_date(d(2026, 3, 20), 14), d(2026, 4, 3));
        assert_eq!(due_date(d(2026, 3, 28), 14), d(2026, 4, 11));
    }

    #[test]
    fn due_date_month_end() {
        assert_eq!(due_date(d(2026, 3, 1), 30), d(2026, 3, 31));
        assert_eq!(due_date(d(2026, 12, 31), 0), d(2026, 12, 31));
    }

    #[test]
    fn fixed_clock_returns_its_date() {
        let clock = FixedClock {
            today: d(2027, 1, 1),
        };
        assert_eq!(clock.today(), d(2027, 1, 1));
        assert_eq!(clock.now_utc().date_naive(), d(2027, 1, 1));
    }

    #[test]
    fn service_date_roundtrips_as_json() {
        let period = ServiceDate::Period {
            from: d(2026, 11, 1),
            to: d(2026, 11, 30),
        };
        let json = serde_json::to_string(&period).unwrap();
        assert_eq!(serde_json::from_str::<ServiceDate>(&json).unwrap(), period);
    }
}
