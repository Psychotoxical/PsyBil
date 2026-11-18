//! Tax categories, coded after EN 16931 (UNCL 5305).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// How a line is taxed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum TaxCategory {
    /// Regular VAT at the given rate in percent (e.g. 19 or 7).
    Standard {
        #[cfg_attr(test, ts(type = "string"))]
        rate: Decimal,
    },
    /// Tax-exempt; the reason is printed on the invoice.
    Exempt { reason: String },
    /// Small business, no VAT shown (§ 19 UStG).
    SmallBusiness,
    /// Reverse charge, the recipient owes the VAT (§ 13b UStG).
    ReverseCharge,
    /// Tax-free intra-community supply.
    IntraCommunity,
}

impl TaxCategory {
    /// EN 16931 category code.
    pub fn code(&self) -> &'static str {
        match self {
            TaxCategory::Standard { .. } => "S",
            TaxCategory::Exempt { .. } | TaxCategory::SmallBusiness => "E",
            TaxCategory::ReverseCharge => "AE",
            TaxCategory::IntraCommunity => "K",
        }
    }

    /// Rate in percent; zero for everything except `Standard`.
    pub fn rate(&self) -> Decimal {
        match self {
            TaxCategory::Standard { rate } => *rate,
            _ => Decimal::ZERO,
        }
    }

    /// Sort key: standard rates first (highest first), then the others in
    /// declaration order.
    pub(crate) fn sort_key(&self) -> (u8, Decimal) {
        match self {
            TaxCategory::Standard { rate } => (0, -*rate),
            TaxCategory::Exempt { .. } => (1, Decimal::ZERO),
            TaxCategory::SmallBusiness => (2, Decimal::ZERO),
            TaxCategory::ReverseCharge => (3, Decimal::ZERO),
            TaxCategory::IntraCommunity => (4, Decimal::ZERO),
        }
    }
}

/// The tax setting of a line: a fixed category, or following the goods on
/// the same invoice (used for shipping as an ancillary service).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "category", rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum TaxChoice {
    Fixed(TaxCategory),
    FollowGoods,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::dec;

    #[test]
    fn codes_follow_en16931() {
        assert_eq!(TaxCategory::Standard { rate: dec!(19) }.code(), "S");
        assert_eq!(TaxCategory::Exempt { reason: "x".into() }.code(), "E");
        assert_eq!(TaxCategory::SmallBusiness.code(), "E");
        assert_eq!(TaxCategory::ReverseCharge.code(), "AE");
        assert_eq!(TaxCategory::IntraCommunity.code(), "K");
    }

    #[test]
    fn only_standard_has_a_rate() {
        assert_eq!(TaxCategory::Standard { rate: dec!(7) }.rate(), dec!(7));
        assert_eq!(TaxCategory::SmallBusiness.rate(), dec!(0));
    }

    #[test]
    fn tax_choice_roundtrips_as_json() {
        let choice = TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(19) });
        let json = serde_json::to_string(&choice).unwrap();
        assert_eq!(serde_json::from_str::<TaxChoice>(&json).unwrap(), choice);
        let follow = serde_json::to_string(&TaxChoice::FollowGoods).unwrap();
        assert_eq!(
            serde_json::from_str::<TaxChoice>(&follow).unwrap(),
            TaxChoice::FollowGoods
        );
    }
}
