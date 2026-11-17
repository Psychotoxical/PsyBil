//! Invoice arithmetic. Every total in PsyBil comes from `calculate`.
//!
//! Line amounts are rounded to cents; tax is computed once per tax category
//! on the sum of its lines (EN 16931, BR-CO-17), never per line.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

use crate::money::{CalcError, Cents, MAX_INPUT_SCALE, cents_to_decimal, round_to_cents};
use crate::tax::{TaxCategory, TaxChoice};

/// Whether unit prices on an invoice are net or gross.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceMode {
    Net,
    Gross,
}

impl PriceMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PriceMode::Net => "net",
            PriceMode::Gross => "gross",
        }
    }

    pub fn parse(text: &str) -> Option<PriceMode> {
        match text {
            "net" => Some(PriceMode::Net),
            "gross" => Some(PriceMode::Gross),
            _ => None,
        }
    }
}

/// What a line represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Item,
    Free,
    Shipping,
    Discount,
}

impl LineKind {
    fn is_goods(self) -> bool {
        matches!(self, LineKind::Item | LineKind::Free)
    }
}

/// The numbers of one line needed for calculation.
#[derive(Debug, Clone, PartialEq)]
pub struct LineInput {
    pub kind: LineKind,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub tax: TaxChoice,
}

/// Sum of all lines in one tax category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxGroup {
    pub category: TaxCategory,
    pub base: Cents,
    pub tax: Cents,
    pub gross: Cents,
}

/// Result of `calculate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    /// Rounded amount of each line, net or gross as the price mode says.
    pub line_amounts: Vec<Cents>,
    pub groups: Vec<TaxGroup>,
    pub net: Cents,
    pub tax: Cents,
    pub gross: Cents,
}

impl Totals {
    /// The same totals with every amount negated, for a cancellation.
    pub fn negated(&self) -> Totals {
        Totals {
            line_amounts: self.line_amounts.iter().map(|a| Cents(-a.0)).collect(),
            groups: self
                .groups
                .iter()
                .map(|g| TaxGroup {
                    category: g.category.clone(),
                    base: Cents(-g.base.0),
                    tax: Cents(-g.tax.0),
                    gross: Cents(-g.gross.0),
                })
                .collect(),
            net: Cents(-self.net.0),
            tax: Cents(-self.tax.0),
            gross: Cents(-self.gross.0),
        }
    }
}

/// Calculates all totals of an invoice.
///
/// With `small_business` every line counts as `SmallBusiness` and no tax is
/// charged.
pub fn calculate(
    lines: &[LineInput],
    mode: PriceMode,
    small_business: bool,
) -> Result<Totals, CalcError> {
    let mut line_amounts = Vec::with_capacity(lines.len());
    for line in lines {
        check_scale(line.quantity)?;
        check_scale(line.unit_price)?;
        let product = line
            .quantity
            .checked_mul(line.unit_price)
            .ok_or(CalcError::Overflow)?;
        line_amounts.push(round_to_cents(product)?);
    }

    // Net weights of the goods per category, in order of first appearance.
    let mut goods: Vec<(TaxCategory, Decimal)> = Vec::new();
    if !small_business {
        for (line, amount) in lines.iter().zip(&line_amounts) {
            if let (true, TaxChoice::Fixed(category)) = (line.kind.is_goods(), &line.tax) {
                let weight = net_weight(*amount, category, mode)?;
                match goods.iter_mut().find(|(c, _)| c == category) {
                    Some((_, sum)) => {
                        *sum = sum.checked_add(weight).ok_or(CalcError::Overflow)?;
                    }
                    None => goods.push((category.clone(), weight)),
                }
            }
        }
    }

    let mut groups: Vec<(TaxCategory, Cents)> = Vec::new();
    for (line, amount) in lines.iter().zip(&line_amounts) {
        if small_business {
            add_to(&mut groups, &TaxCategory::SmallBusiness, *amount)?;
            continue;
        }
        match &line.tax {
            TaxChoice::Fixed(category) => add_to(&mut groups, category, *amount)?,
            TaxChoice::FollowGoods => {
                for (category, share) in split_by_goods(*amount, &goods)? {
                    add_to(&mut groups, &category, share)?;
                }
            }
        }
    }

    let mut result = Vec::with_capacity(groups.len());
    let (mut net, mut tax) = (Cents::ZERO, Cents::ZERO);
    for (category, sum) in groups {
        let rate = checked_rate(&category)?;
        let amount = cents_to_decimal(sum);
        let group = match mode {
            PriceMode::Net => {
                let tax = round_to_cents(
                    amount
                        .checked_mul(rate)
                        .and_then(|v| v.checked_div(Decimal::ONE_HUNDRED))
                        .ok_or(CalcError::Overflow)?,
                )?;
                TaxGroup {
                    category,
                    base: sum,
                    tax,
                    gross: sum.checked_add(tax)?,
                }
            }
            PriceMode::Gross => {
                let tax = round_to_cents(
                    amount
                        .checked_mul(rate)
                        .and_then(|v| v.checked_div(Decimal::ONE_HUNDRED + rate))
                        .ok_or(CalcError::Overflow)?,
                )?;
                TaxGroup {
                    category,
                    base: sum.checked_sub(tax)?,
                    tax,
                    gross: sum,
                }
            }
        };
        net = net.checked_add(group.base)?;
        tax = tax.checked_add(group.tax)?;
        result.push(group);
    }
    result.sort_by_key(|group| group.category.sort_key());

    Ok(Totals {
        line_amounts,
        groups: result,
        net,
        tax,
        gross: net.checked_add(tax)?,
    })
}

fn check_scale(value: Decimal) -> Result<(), CalcError> {
    if value.normalize().scale() > MAX_INPUT_SCALE {
        return Err(CalcError::TooManyDecimals);
    }
    Ok(())
}

fn add_to(
    groups: &mut Vec<(TaxCategory, Cents)>,
    category: &TaxCategory,
    amount: Cents,
) -> Result<(), CalcError> {
    match groups.iter_mut().find(|(c, _)| c == category) {
        Some((_, sum)) => *sum = sum.checked_add(amount)?,
        None => groups.push((category.clone(), amount)),
    }
    Ok(())
}

/// The rate of a category, checked to lie between 0 and 100 %.
fn checked_rate(category: &TaxCategory) -> Result<Decimal, CalcError> {
    let rate = category.rate();
    if rate < Decimal::ZERO || rate > Decimal::ONE_HUNDRED {
        return Err(CalcError::InvalidRate);
    }
    Ok(rate)
}

/// The net value of a line amount, used to weigh the shipping split
/// ("Waren-Netto"). In gross mode the tax is taken out without rounding.
fn net_weight(
    amount: Cents,
    category: &TaxCategory,
    mode: PriceMode,
) -> Result<Decimal, CalcError> {
    let value = Decimal::from(amount.0);
    match mode {
        PriceMode::Net => Ok(value),
        PriceMode::Gross => {
            let rate = checked_rate(category)?;
            value
                .checked_mul(Decimal::ONE_HUNDRED)
                .and_then(|v| v.checked_div(Decimal::ONE_HUNDRED + rate))
                .ok_or(CalcError::Overflow)
        }
    }
}

/// Splits an amount over the goods categories in proportion to their net
/// values. The rounding remainder goes to the largest share. Without goods
/// the amount is taxed at 19 %.
fn split_by_goods(
    amount: Cents,
    goods: &[(TaxCategory, Decimal)],
) -> Result<Vec<(TaxCategory, Cents)>, CalcError> {
    if goods.is_empty() {
        let default = TaxCategory::Standard {
            rate: Decimal::from(19),
        };
        return Ok(vec![(default, amount)]);
    }
    let mut total = Decimal::ZERO;
    for (_, weight) in goods {
        total = total.checked_add(*weight).ok_or(CalcError::Overflow)?;
    }
    if goods.len() == 1 || total.is_zero() {
        return Ok(vec![(goods[0].0.clone(), amount)]);
    }
    let mut shares = Vec::with_capacity(goods.len());
    let mut assigned = Cents::ZERO;
    for (category, weight) in goods {
        let share = Decimal::from(amount.0)
            .checked_mul(*weight)
            .and_then(|v| v.checked_div(total))
            .ok_or(CalcError::Overflow)?;
        let share = share.round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero);
        let share = Cents(share.to_i64().ok_or(CalcError::Overflow)?);
        assigned = assigned.checked_add(share)?;
        shares.push((category.clone(), share));
    }
    let remainder = amount.checked_sub(assigned)?;
    if remainder != Cents::ZERO {
        let largest = goods
            .iter()
            .enumerate()
            .max_by_key(|(_, (_, w))| w.abs())
            .map(|(i, _)| i)
            .unwrap_or(0);
        shares[largest].1 = shares[largest].1.checked_add(remainder)?;
    }
    Ok(shares)
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use rust_decimal::dec;

    fn line(kind: LineKind, quantity: Decimal, unit_price: Decimal, tax: TaxChoice) -> LineInput {
        LineInput {
            kind,
            quantity,
            unit_price,
            tax,
        }
    }

    fn std_rate(rate: Decimal) -> TaxChoice {
        TaxChoice::Fixed(TaxCategory::Standard { rate })
    }

    #[test]
    fn follow_goods_split_overflow_is_error() {
        let lines = vec![
            line(
                LineKind::Item,
                dec!(1000000),
                dec!(1000000000),
                std_rate(dec!(19)),
            ),
            line(LineKind::Item, dec!(1), dec!(1), std_rate(dec!(7))),
            line(
                LineKind::Shipping,
                dec!(1),
                dec!(10000000000),
                TaxChoice::FollowGoods,
            ),
        ];
        assert_eq!(
            calculate(&lines, PriceMode::Net, false),
            Err(CalcError::Overflow)
        );
    }

    #[test]
    fn invalid_rate_is_error() {
        let lines = vec![line(
            LineKind::Item,
            dec!(1),
            dec!(10),
            std_rate(dec!(-100)),
        )];
        assert_eq!(
            calculate(&lines, PriceMode::Gross, false),
            Err(CalcError::InvalidRate)
        );
        let lines = vec![line(LineKind::Item, dec!(1), dec!(10), std_rate(dec!(150)))];
        assert_eq!(
            calculate(&lines, PriceMode::Net, false),
            Err(CalcError::InvalidRate)
        );
    }

    #[test]
    fn gross_mode_follow_goods_splits_by_net() {
        let lines = vec![
            line(LineKind::Item, dec!(1), dec!(119), std_rate(dec!(19))),
            line(LineKind::Item, dec!(1), dec!(107), std_rate(dec!(7))),
            line(
                LineKind::Shipping,
                dec!(1),
                dec!(10),
                TaxChoice::FollowGoods,
            ),
        ];
        let t = calculate(&lines, PriceMode::Gross, false).unwrap();
        assert_eq!(t.groups[0].gross, Cents(12400));
        assert_eq!(t.groups[1].gross, Cents(11200));
        assert_eq!(t.tax, Cents(2713));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tax::{TaxCategory, TaxChoice};
    use rust_decimal::dec;

    fn std_rate(rate: Decimal) -> TaxChoice {
        TaxChoice::Fixed(TaxCategory::Standard { rate })
    }

    fn line(kind: LineKind, quantity: Decimal, unit_price: Decimal, tax: TaxChoice) -> LineInput {
        LineInput {
            kind,
            quantity,
            unit_price,
            tax,
        }
    }

    fn item(quantity: Decimal, unit_price: Decimal, rate: Decimal) -> LineInput {
        line(LineKind::Item, quantity, unit_price, std_rate(rate))
    }

    #[test]
    fn per_rate_rounding_three_lines() {
        let lines = vec![
            item(dec!(1), dec!(0.33), dec!(19)),
            item(dec!(1), dec!(0.33), dec!(19)),
            item(dec!(1), dec!(0.33), dec!(19)),
        ];
        let t = calculate(&lines, PriceMode::Net, false).unwrap();
        assert_eq!(t.tax, Cents(19));
        assert_eq!(t.gross, Cents(118));
    }

    #[test]
    fn mixed_rates() {
        let lines = vec![
            item(dec!(1), dec!(0.33), dec!(19)),
            item(dec!(1), dec!(0.33), dec!(19)),
            item(dec!(1), dec!(0.33), dec!(19)),
            item(dec!(2), dec!(1.07), dec!(7)),
        ];
        let t = calculate(&lines, PriceMode::Net, false).unwrap();
        assert_eq!(t.groups.len(), 2);
        assert_eq!(
            t.groups[0].category,
            TaxCategory::Standard { rate: dec!(19) }
        );
        assert_eq!((t.groups[0].base, t.groups[0].tax), (Cents(99), Cents(19)));
        assert_eq!(
            t.groups[1].category,
            TaxCategory::Standard { rate: dec!(7) }
        );
        assert_eq!((t.groups[1].base, t.groups[1].tax), (Cents(214), Cents(15)));
        assert_eq!(t.gross, Cents(347));
        assert_eq!(t.net + t.tax, t.gross);
    }

    #[test]
    fn gross_mode_999() {
        let lines = vec![item(dec!(3), dec!(9.99), dec!(19))];
        let t = calculate(&lines, PriceMode::Gross, false).unwrap();
        assert_eq!(t.gross, Cents(2997));
        assert_eq!(t.tax, Cents(479));
        assert_eq!(t.net, Cents(2518));
        assert_eq!(t.line_amounts, vec![Cents(2997)]);
    }

    #[test]
    fn shipping_exempt_zero() {
        let lines = vec![
            item(dec!(1), dec!(100), dec!(19)),
            line(
                LineKind::Shipping,
                dec!(1),
                dec!(4.90),
                TaxChoice::Fixed(TaxCategory::Exempt {
                    reason: "Befreit".into(),
                }),
            ),
        ];
        let t = calculate(&lines, PriceMode::Net, false).unwrap();
        assert_eq!(t.groups.len(), 2);
        assert_eq!(
            (t.groups[0].base, t.groups[0].tax),
            (Cents(10000), Cents(1900))
        );
        assert_eq!(t.groups[1].category.code(), "E");
        assert_eq!((t.groups[1].base, t.groups[1].tax), (Cents(490), Cents(0)));
        assert_eq!(t.gross, Cents(12390));
    }

    #[test]
    fn shipping_follows_mixed_goods() {
        let lines = vec![
            item(dec!(1), dec!(100), dec!(19)),
            item(dec!(1), dec!(50), dec!(7)),
            line(
                LineKind::Shipping,
                dec!(1),
                dec!(6.00),
                TaxChoice::FollowGoods,
            ),
        ];
        let t = calculate(&lines, PriceMode::Net, false).unwrap();
        assert_eq!(
            (t.groups[0].base, t.groups[0].tax),
            (Cents(10400), Cents(1976))
        );
        assert_eq!(
            (t.groups[1].base, t.groups[1].tax),
            (Cents(5200), Cents(364))
        );
    }

    #[test]
    fn shipping_follows_single_goods_rate() {
        let lines = vec![
            item(dec!(1), dec!(10), dec!(7)),
            line(LineKind::Shipping, dec!(1), dec!(5), TaxChoice::FollowGoods),
        ];
        let t = calculate(&lines, PriceMode::Net, false).unwrap();
        assert_eq!(t.groups.len(), 1);
        assert_eq!(t.groups[0].base, Cents(1500));
    }

    #[test]
    fn shipping_without_goods_defaults_to_19() {
        let lines = vec![line(
            LineKind::Shipping,
            dec!(1),
            dec!(10),
            TaxChoice::FollowGoods,
        )];
        let t = calculate(&lines, PriceMode::Net, false).unwrap();
        assert_eq!(
            t.groups[0].category,
            TaxCategory::Standard { rate: dec!(19) }
        );
        assert_eq!(t.tax, Cents(190));
    }

    #[test]
    fn split_remainder_goes_to_largest_share() {
        let lines = vec![
            item(dec!(1), dec!(2), dec!(19)),
            item(dec!(1), dec!(1), dec!(7)),
            line(
                LineKind::Shipping,
                dec!(1),
                dec!(0.01),
                TaxChoice::FollowGoods,
            ),
        ];
        let t = calculate(&lines, PriceMode::Net, false).unwrap();
        assert_eq!(t.groups[0].base, Cents(201));
        assert_eq!(t.groups[1].base, Cents(100));
    }

    #[test]
    fn credit_note_negative() {
        let lines = vec![item(dec!(-1), dec!(100), dec!(19))];
        let t = calculate(&lines, PriceMode::Net, false).unwrap();
        assert_eq!(t.net, Cents(-10000));
        assert_eq!(t.tax, Cents(-1900));
        assert_eq!(t.gross, Cents(-11900));
    }

    #[test]
    fn small_business_ignores_categories() {
        let lines = vec![item(dec!(2), dec!(12.50), dec!(19))];
        let t = calculate(&lines, PriceMode::Net, true).unwrap();
        assert_eq!(t.groups.len(), 1);
        assert_eq!(t.groups[0].category, TaxCategory::SmallBusiness);
        assert_eq!(t.tax, Cents(0));
        assert_eq!(t.gross, t.net);
        assert_eq!(t.gross, Cents(2500));
    }

    #[test]
    fn zero_rate_and_zero_quantity_are_valid() {
        let lines = vec![item(dec!(0), dec!(5), dec!(0))];
        let t = calculate(&lines, PriceMode::Net, false).unwrap();
        assert_eq!((t.net, t.tax, t.gross), (Cents(0), Cents(0), Cents(0)));
    }

    #[test]
    fn too_many_decimals_rejected() {
        let lines = vec![item(dec!(1.23456), dec!(1), dec!(19))];
        assert_eq!(
            calculate(&lines, PriceMode::Net, false),
            Err(CalcError::TooManyDecimals)
        );
    }

    #[test]
    fn huge_amount_overflows() {
        let lines = vec![item(dec!(1000000000), dec!(1000000000000), dec!(19))];
        assert_eq!(
            calculate(&lines, PriceMode::Net, false),
            Err(CalcError::Overflow)
        );
    }
}
