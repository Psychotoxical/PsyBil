//! Texts and formats of the invoice language (German or English).

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::money::Cents;
use crate::tax::TaxCategory;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

impl Lang {
    /// `"en"` (also `"en-GB"` and the like) is English, everything else German.
    pub fn from_code(code: &str) -> Lang {
        if code.trim().to_lowercase().starts_with("en") {
            Lang::En
        } else {
            Lang::De
        }
    }
}

/// All fixed texts of the PDF. `{…}` marks are filled with `fill`.
pub(crate) struct Labels {
    pub title_invoice: &'static str,
    pub title_storno: &'static str,
    pub draft_mark: &'static str,
    pub draft_number: &'static str,
    pub number: &'static str,
    pub invoice_date: &'static str,
    pub service_date: &'static str,
    pub service_period: &'static str,
    pub same_as_invoice: &'static str,
    pub due: &'static str,
    pub cancels: &'static str,
    pub cancels_value: &'static str,
    pub customer_vat: &'static str,
    pub col_pos: &'static str,
    pub col_desc: &'static str,
    pub col_qty: &'static str,
    pub col_unit: &'static str,
    pub col_vat: &'static str,
    pub col_unit_price: &'static str,
    pub col_amount: &'static str,
    pub net: &'static str,
    pub gross: &'static str,
    pub subtotal_net: &'static str,
    pub plus_vat: &'static str,
    pub incl_vat: &'static str,
    pub total: &'static str,
    pub paid_before: &'static str,
    pub still_open: &'static str,
    pub pay_transfer: &'static str,
    pub pay_by: &'static str,
    pub paid: &'static str,
    pub page: &'static str,
    pub imported: &'static str,
    pub tax_number: &'static str,
    pub vat_id: &'static str,
    pub phone: &'static str,
    pub email: &'static str,
    pub web: &'static str,
    pub iban: &'static str,
    pub bic: &'static str,
}

static DE: Labels = Labels {
    title_invoice: "Rechnung",
    title_storno: "Stornorechnung",
    draft_mark: "ENTWURF",
    draft_number: "Entwurf",
    number: "Rechnungsnummer",
    invoice_date: "Rechnungsdatum",
    service_date: "Leistungsdatum",
    service_period: "Leistungszeitraum",
    same_as_invoice: "entspricht Rechnungsdatum",
    due: "Fällig am",
    cancels: "Storno zu",
    cancels_value: "{number} vom {date}",
    customer_vat: "USt-IdNr. Kunde",
    col_pos: "Pos.",
    col_desc: "Bezeichnung",
    col_qty: "Menge",
    col_unit: "Einheit",
    col_vat: "USt",
    col_unit_price: "Einzelpreis",
    col_amount: "Gesamt",
    net: "netto",
    gross: "brutto",
    subtotal_net: "Summe netto",
    plus_vat: "zzgl. USt {rate} auf {base}",
    incl_vat: "enthaltene USt {rate} auf {base}",
    total: "Gesamtbetrag",
    paid_before: "bereits bezahlt",
    still_open: "noch offen",
    pay_transfer: "Bitte überweisen Sie {amount} bis zum {due} auf das unten angegebene Konto. \
                   Verwendungszweck: {number}.",
    pay_by: "Zahlbar bis zum {due}.",
    paid: "Betrag dankend erhalten.",
    page: "Seite {x} von {y}",
    imported: "Nachträglich erzeugt aus übernommenen Daten.",
    tax_number: "Steuernummer",
    vat_id: "USt-IdNr.",
    phone: "Tel.",
    email: "E-Mail",
    web: "Web",
    iban: "IBAN",
    bic: "BIC",
};

static EN: Labels = Labels {
    title_invoice: "Invoice",
    title_storno: "Cancellation invoice",
    draft_mark: "DRAFT",
    draft_number: "Draft",
    number: "Invoice number",
    invoice_date: "Invoice date",
    service_date: "Date of service",
    service_period: "Service period",
    same_as_invoice: "same as invoice date",
    due: "Due date",
    cancels: "Cancels",
    cancels_value: "{number} of {date}",
    customer_vat: "Customer VAT ID",
    col_pos: "No.",
    col_desc: "Description",
    col_qty: "Qty",
    col_unit: "Unit",
    col_vat: "VAT",
    col_unit_price: "Unit price",
    col_amount: "Amount",
    net: "net",
    gross: "gross",
    subtotal_net: "Subtotal (net)",
    plus_vat: "plus VAT {rate} on {base}",
    incl_vat: "incl. VAT {rate} on {base}",
    total: "Total",
    paid_before: "already paid",
    still_open: "amount due",
    pay_transfer: "Please transfer {amount} by {due} to the account below. Reference: {number}.",
    pay_by: "Payable by {due}.",
    paid: "Amount received with thanks.",
    page: "Page {x} of {y}",
    imported: "Generated afterwards from imported data.",
    tax_number: "Tax number",
    vat_id: "VAT ID",
    phone: "Phone",
    email: "Email",
    web: "Web",
    iban: "IBAN",
    bic: "BIC",
};

impl Labels {
    pub(crate) fn of(lang: Lang) -> &'static Labels {
        match lang {
            Lang::De => &DE,
            Lang::En => &EN,
        }
    }
}

/// Puts the values into the `{name}` marks of `template`.
pub(crate) fn fill(template: &str, values: &[(&str, &str)]) -> String {
    values
        .iter()
        .fold(template.to_string(), |text, (name, value)| {
            text.replace(&format!("{{{name}}}"), value)
        })
}

/// Digits of a whole number with thousands separators.
fn grouped(digits: &str, separator: char) -> String {
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(c);
    }
    out
}

fn separators(lang: Lang) -> (char, char) {
    match lang {
        Lang::De => ('.', ','),
        Lang::En => (',', '.'),
    }
}

pub(crate) fn money(amount: Cents, currency: &str, lang: Lang) -> String {
    let (thousands, decimal) = separators(lang);
    let abs = amount.0.unsigned_abs();
    let number = format!(
        "{}{decimal}{:02}",
        grouped(&(abs / 100).to_string(), thousands),
        abs % 100
    );
    with_currency(amount.0 < 0, &number, currency, lang)
}

fn with_currency(negative: bool, number: &str, currency: &str, lang: Lang) -> String {
    let sign = if negative { "-" } else { "" };
    let symbol = match currency {
        "EUR" => Some("€"),
        "USD" => Some("$"),
        "GBP" => Some("£"),
        _ => None,
    };
    match (lang, symbol) {
        (Lang::De, Some(s)) => format!("{sign}{number} {s}"),
        (Lang::De, None) => format!("{sign}{number} {currency}"),
        (Lang::En, Some(s)) => format!("{sign}{s}{number}"),
        (Lang::En, None) => format!("{sign}{currency} {number}"),
    }
}

/// A unit price: at least two decimals, more only where the price has them.
pub(crate) fn price(value: Decimal, currency: &str, lang: Lang) -> String {
    let (thousands, decimal) = separators(lang);
    let text = value.round_dp(4).normalize().abs().to_string();
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    let number = format!("{}{decimal}{fraction:0<2}", grouped(whole, thousands));
    with_currency(
        value.is_sign_negative() && !value.is_zero(),
        &number,
        currency,
        lang,
    )
}

pub(crate) fn date(d: NaiveDate, lang: Lang) -> String {
    match lang {
        Lang::De => d.format("%d.%m.%Y").to_string(),
        Lang::En => d.format("%-d %b %Y").to_string(),
    }
}

/// A decimal without trailing zeros, with the separators of `lang`.
fn decimal(value: Decimal, lang: Lang) -> String {
    let (thousands, decimal) = separators(lang);
    let text = value.normalize().abs().to_string();
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    let sign = if value.is_sign_negative() && !value.is_zero() {
        "-"
    } else {
        ""
    };
    let whole = grouped(whole, thousands);
    if fraction.is_empty() {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}{decimal}{fraction}")
    }
}

pub(crate) fn quantity(q: Decimal, lang: Lang) -> String {
    decimal(q, lang)
}

pub(crate) fn rate(r: Decimal, lang: Lang) -> String {
    match lang {
        Lang::De => format!("{} %", decimal(r, lang)),
        Lang::En => format!("{}%", decimal(r, lang)),
    }
}

/// The note a tax category needs on the invoice, if any.
pub(crate) fn tax_note(category: &TaxCategory, lang: Lang) -> Option<String> {
    let text = match (category, lang) {
        (TaxCategory::Standard { .. }, _) => return None,
        (TaxCategory::Exempt { reason }, _) => return Some(reason.clone()),
        (TaxCategory::SmallBusiness, Lang::De) => {
            "Gemäß § 19 UStG wird keine Umsatzsteuer berechnet."
        }
        (TaxCategory::SmallBusiness, Lang::En) => {
            "No VAT is charged under the small business scheme (§ 19 UStG)."
        }
        (TaxCategory::ReverseCharge, Lang::De) => {
            "Steuerschuldnerschaft des Leistungsempfängers (§ 13b UStG)."
        }
        (TaxCategory::ReverseCharge, Lang::En) => {
            "Reverse charge: the recipient is liable for VAT (§ 13b UStG)."
        }
        (TaxCategory::IntraCommunity, Lang::De) => {
            "Steuerfreie innergemeinschaftliche Lieferung (§ 4 Nr. 1b UStG)."
        }
        (TaxCategory::IntraCommunity, Lang::En) => {
            "Tax-exempt intra-community supply (§ 4 No. 1b UStG)."
        }
    };
    Some(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::d;
    use rust_decimal::dec;

    #[test]
    fn money_formats_de_en_and_negative() {
        use Lang::{De, En};
        assert_eq!(money(Cents(123456), "EUR", De), "1.234,56 €");
        assert_eq!(money(Cents(123456), "EUR", En), "€1,234.56");
        assert_eq!(money(Cents(-2975), "EUR", De), "-29,75 €");
        assert_eq!(money(Cents(-2975), "EUR", En), "-€29.75");
        assert_eq!(money(Cents(123456), "USD", En), "$1,234.56");
        assert_eq!(money(Cents(123456), "GBP", De), "1.234,56 £");
        assert_eq!(money(Cents(123456), "CHF", De), "1.234,56 CHF");
        assert_eq!(money(Cents(123456), "CHF", En), "CHF 1,234.56");
        assert_eq!(money(Cents(5), "EUR", De), "0,05 €");
        assert_eq!(money(Cents(123456789), "EUR", De), "1.234.567,89 €");
    }

    #[test]
    fn unit_price_keeps_needed_decimals() {
        assert_eq!(price(dec!(12.5), "EUR", Lang::De), "12,50 €");
        assert_eq!(price(dec!(0.125), "EUR", Lang::De), "0,125 €");
        assert_eq!(price(dec!(1234), "EUR", Lang::En), "€1,234.00");
        assert_eq!(price(dec!(-5), "EUR", Lang::De), "-5,00 €");
    }

    #[test]
    fn dates_and_quantities() {
        assert_eq!(date(d(2026, 11, 18), Lang::De), "18.11.2026");
        assert_eq!(date(d(2026, 11, 18), Lang::En), "18 Nov 2026");
        assert_eq!(date(d(2026, 3, 1), Lang::De), "01.03.2026");
        assert_eq!(date(d(2026, 3, 1), Lang::En), "1 Mar 2026");
        assert_eq!(quantity(dec!(1.5), Lang::De), "1,5");
        assert_eq!(quantity(dec!(2.000), Lang::De), "2");
        assert_eq!(quantity(dec!(-2), Lang::De), "-2");
        assert_eq!(quantity(dec!(1000), Lang::De), "1.000");
        assert_eq!(quantity(dec!(1.5), Lang::En), "1.5");
        assert_eq!(quantity(dec!(1000), Lang::En), "1,000");
    }

    #[test]
    fn rates() {
        assert_eq!(rate(dec!(19), Lang::De), "19 %");
        assert_eq!(rate(dec!(7.00), Lang::De), "7 %");
        assert_eq!(rate(dec!(16.5), Lang::De), "16,5 %");
        assert_eq!(rate(dec!(19), Lang::En), "19%");
        assert_eq!(rate(dec!(16.5), Lang::En), "16.5%");
    }

    #[test]
    fn tax_notes_per_category() {
        use Lang::{De, En};
        assert_eq!(
            tax_note(&TaxCategory::SmallBusiness, De).as_deref(),
            Some("Gemäß § 19 UStG wird keine Umsatzsteuer berechnet.")
        );
        assert_eq!(
            tax_note(&TaxCategory::SmallBusiness, En).as_deref(),
            Some("No VAT is charged under the small business scheme (§ 19 UStG).")
        );
        assert_eq!(
            tax_note(&TaxCategory::ReverseCharge, De).as_deref(),
            Some("Steuerschuldnerschaft des Leistungsempfängers (§ 13b UStG).")
        );
        assert_eq!(
            tax_note(&TaxCategory::IntraCommunity, En).as_deref(),
            Some("Tax-exempt intra-community supply (§ 4 No. 1b UStG).")
        );
        assert_eq!(
            tax_note(
                &TaxCategory::Exempt {
                    reason: "Steuerfrei nach § 4 Nr. 20 UStG".into()
                },
                De
            )
            .as_deref(),
            Some("Steuerfrei nach § 4 Nr. 20 UStG")
        );
        assert_eq!(
            tax_note(&TaxCategory::Standard { rate: dec!(19) }, De),
            None
        );
    }

    #[test]
    fn unknown_language_is_german() {
        assert_eq!(Lang::from_code("fr"), Lang::De);
        assert_eq!(Lang::from_code(""), Lang::De);
        assert_eq!(Lang::from_code("en-GB"), Lang::En);
        assert_eq!(Labels::of(Lang::from_code("xx")).title_invoice, "Rechnung");
        assert_eq!(Labels::of(Lang::En).title_storno, "Cancellation invoice");
        assert_eq!(
            fill(Labels::of(Lang::De).page, &[("x", "1"), ("y", "3")]),
            "Seite 1 von 3"
        );
    }
}
