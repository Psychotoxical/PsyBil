//! CSV files for spreadsheets: the invoice list and the year summary.
//!
//! German files use `;` and a decimal comma, English ones `,` and a decimal
//! point, so Excel and LibreOffice open them in the right columns. Files are
//! UTF-8 with BOM and CRLF line ends; text that starts like a formula gets a
//! leading `'`.

use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension};
use rust_decimal::Decimal;

use crate::Result;
use crate::calc::{TaxGroup, Totals};
use crate::dates::ServiceDate;
use crate::invoice::{InvoiceKind, InvoiceState};
use crate::listing::{InvoiceFilter, InvoiceRow, list_invoices};
use crate::money::Cents;
use crate::payments::StatusKind;
use crate::pdf::Lang;
use crate::tax::TaxCategory;
use crate::year_summary::YearSummary;

fn separator(lang: Lang) -> char {
    match lang {
        Lang::De => ';',
        Lang::En => ',',
    }
}

/// One text cell: quoted when needed, guarded against formulas.
pub(crate) fn text_cell(value: &str, lang: Lang) -> String {
    let guarded = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    if guarded.contains([separator(lang), '"', '\r', '\n']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

/// An amount without thousands separators: `1234,56` or `1234.56`.
pub(crate) fn amount_cell(value: Cents, lang: Lang) -> String {
    let decimal = match lang {
        Lang::De => ',',
        Lang::En => '.',
    };
    let sign = if value.0 < 0 { "-" } else { "" };
    let abs = value.0.unsigned_abs();
    format!("{sign}{}{decimal}{:02}", abs / 100, abs % 100)
}

pub(crate) fn date_cell(value: NaiveDate, lang: Lang) -> String {
    match lang {
        Lang::De => value.format("%d.%m.%Y").to_string(),
        Lang::En => value.format("%Y-%m-%d").to_string(),
    }
}

/// Joins cells and rows the CSV way: BOM first, CRLF after every row.
fn write_rows(rows: &[Vec<String>], lang: Lang) -> String {
    let sep = separator(lang).to_string();
    let mut out = String::from('\u{feff}');
    for row in rows {
        out.push_str(&row.join(&sep));
        out.push_str("\r\n");
    }
    out
}

/// A tax column group, in the order the file shows them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum TaxColumn {
    /// Sorted by the negated rate, so higher rates come first.
    Standard(std::cmp::Reverse<Decimal>),
    Exempt,
    SmallBusiness,
    ReverseCharge,
    IntraCommunity,
}

impl TaxColumn {
    fn of(category: &TaxCategory) -> TaxColumn {
        match category {
            TaxCategory::Standard { rate } => TaxColumn::Standard(std::cmp::Reverse(*rate)),
            TaxCategory::Exempt { .. } => TaxColumn::Exempt,
            TaxCategory::SmallBusiness => TaxColumn::SmallBusiness,
            TaxCategory::ReverseCharge => TaxColumn::ReverseCharge,
            TaxCategory::IntraCommunity => TaxColumn::IntraCommunity,
        }
    }

    fn headers(&self, lang: Lang) -> Vec<String> {
        let net = match lang {
            Lang::De => "Netto",
            Lang::En => "Net",
        };
        match (self, lang) {
            (TaxColumn::Standard(rate), _) => {
                let rate = crate::pdf::locale::rate(rate.0, lang);
                let tax = match lang {
                    Lang::De => "Steuer",
                    Lang::En => "VAT",
                };
                vec![format!("{net} {rate}"), format!("{tax} {rate}")]
            }
            (TaxColumn::Exempt, Lang::De) => vec!["Netto steuerfrei".into()],
            (TaxColumn::Exempt, Lang::En) => vec!["Net exempt".into()],
            (TaxColumn::SmallBusiness, Lang::De) => vec!["Netto § 19".into()],
            (TaxColumn::SmallBusiness, Lang::En) => vec!["Net small business".into()],
            (TaxColumn::ReverseCharge, Lang::De) => vec!["Netto Reverse Charge".into()],
            (TaxColumn::ReverseCharge, Lang::En) => vec!["Net reverse charge".into()],
            (TaxColumn::IntraCommunity, Lang::De) => vec!["Netto innergemeinschaftlich".into()],
            (TaxColumn::IntraCommunity, Lang::En) => vec!["Net intra-community".into()],
        }
    }
}

/// The tax groups of a document: frozen ones for issued documents.
fn groups(conn: &Connection, row: &InvoiceRow) -> Result<Vec<TaxGroup>> {
    if row.state == InvoiceState::Draft {
        return Ok(crate::invoice::draft_totals(conn, row.id)?.groups);
    }
    let snapshot: Option<String> = conn
        .query_row(
            "SELECT snapshot FROM invoice_snapshots WHERE invoice_id = ?1",
            [row.id.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    let Some(snapshot) = snapshot else {
        return Ok(Vec::new());
    };
    let value: serde_json::Value = serde_json::from_str(&snapshot)?;
    Ok(serde_json::from_value::<Totals>(value["totals"].clone())
        .map(|t| t.groups)
        .unwrap_or_default())
}

fn status_word(row: &InvoiceRow, lang: Lang) -> String {
    let word = match (row.kind, row.status.kind, lang) {
        (InvoiceKind::Storno, _, Lang::De) => "Storno",
        (InvoiceKind::Storno, _, Lang::En) => "cancellation",
        (_, StatusKind::Draft, Lang::De) => "Entwurf",
        (_, StatusKind::Draft, Lang::En) => "draft",
        (_, StatusKind::Open, Lang::De) => "offen",
        (_, StatusKind::Open, Lang::En) => "open",
        (_, StatusKind::PartlyPaid, Lang::De) => "teilbezahlt",
        (_, StatusKind::PartlyPaid, Lang::En) => "partly paid",
        (_, StatusKind::Paid, Lang::De) => "bezahlt",
        (_, StatusKind::Paid, Lang::En) => "paid",
        (_, StatusKind::Overpaid, Lang::De) => "überbezahlt",
        (_, StatusKind::Overpaid, Lang::En) => "overpaid",
        (_, StatusKind::Cancelled, Lang::De) => "storniert",
        (_, StatusKind::Cancelled, Lang::En) => "cancelled",
    };
    match (row.status.overdue, lang) {
        (true, Lang::De) => format!("{word}, überfällig"),
        (true, Lang::En) => format!("{word}, overdue"),
        (false, _) => word.to_string(),
    }
}

fn service_cell(service: &ServiceDate, invoice_date: NaiveDate, lang: Lang) -> String {
    match service {
        ServiceDate::SameAsInvoice => date_cell(invoice_date, lang),
        ServiceDate::Day { date } => date_cell(*date, lang),
        ServiceDate::Period { from, to } => {
            format!("{} – {}", date_cell(*from, lang), date_cell(*to, lang))
        }
        ServiceDate::None => String::new(),
    }
}

/// The invoices `filter` selects, as the list shows them.
pub fn invoices_csv(
    conn: &Connection,
    filter: &InvoiceFilter,
    today: NaiveDate,
    lang: Lang,
) -> Result<String> {
    let rows = list_invoices(conn, filter, today)?;
    let mut per_row = Vec::with_capacity(rows.len());
    let mut columns: Vec<TaxColumn> = Vec::new();
    for row in &rows {
        let groups = groups(conn, row)?;
        for g in &groups {
            let column = TaxColumn::of(&g.category);
            if !columns.contains(&column) {
                columns.push(column);
            }
        }
        per_row.push(groups);
    }
    columns.sort();
    let mut header: Vec<String> = match lang {
        Lang::De => [
            "Nummer",
            "Datum",
            "Leistungsdatum",
            "Unternehmen",
            "Marke",
            "Kunde",
        ],
        Lang::En => [
            "Number",
            "Date",
            "Service date",
            "Business",
            "Brand",
            "Customer",
        ],
    }
    .iter()
    .map(|s| s.to_string())
    .collect();
    for column in &columns {
        header.extend(column.headers(lang));
    }
    header.extend(
        match lang {
            Lang::De => ["Brutto", "Währung", "Status", "bezahlt", "offen"],
            Lang::En => ["Gross", "Currency", "Status", "Paid", "Open"],
        }
        .iter()
        .map(|s| s.to_string()),
    );
    let mut out = vec![header.iter().map(|h| text_cell(h, lang)).collect()];
    for (row, groups) in rows.iter().zip(per_row) {
        let invoice = crate::invoice::load_invoice(conn, row.id)?;
        let business = crate::masterdata::get_business(conn, row.business_id)?;
        let brand = crate::masterdata::get_brand(conn, row.brand_id)?;
        let mut cells = vec![
            text_cell(row.number.as_deref().unwrap_or(""), lang),
            date_cell(row.invoice_date, lang),
            service_cell(&invoice.service_date, invoice.invoice_date, lang),
            text_cell(&business.name, lang),
            text_cell(&brand.name, lang),
            text_cell(row.customer_name.as_deref().unwrap_or(""), lang),
        ];
        for column in &columns {
            let mut base = Cents::ZERO;
            let mut tax = Cents::ZERO;
            let mut present = false;
            for g in groups
                .iter()
                .filter(|g| &TaxColumn::of(&g.category) == column)
            {
                base = base.checked_add(g.base)?;
                tax = tax.checked_add(g.tax)?;
                present = true;
            }
            let show = |c: Cents| {
                if present {
                    amount_cell(c, lang)
                } else {
                    String::new()
                }
            };
            cells.push(show(base));
            if matches!(column, TaxColumn::Standard(_)) {
                cells.push(show(tax));
            }
        }
        cells.push(amount_cell(row.gross, lang));
        cells.push(text_cell(&row.currency, lang));
        cells.push(text_cell(&status_word(row, lang), lang));
        let settles = row.state == InvoiceState::Issued && row.kind == InvoiceKind::Invoice;
        if settles {
            let paid: i64 = conn.query_row(
                "SELECT COALESCE(SUM(amount_cents), 0) FROM payments
                 WHERE invoice_id = ?1 AND transferred_to_id IS NULL",
                [row.id.to_string()],
                |r| r.get(0),
            )?;
            cells.push(amount_cell(Cents(paid), lang));
            cells.push(amount_cell(row.status.open, lang));
        } else {
            cells.push(String::new());
            cells.push(String::new());
        }
        out.push(cells);
    }
    Ok(write_rows(&out, lang))
}

const MONTHS_DE: [&str; 12] = [
    "Januar",
    "Februar",
    "März",
    "April",
    "Mai",
    "Juni",
    "Juli",
    "August",
    "September",
    "Oktober",
    "November",
    "Dezember",
];
const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// The name of month 1 to 12.
pub(crate) fn month_name(month: u32, lang: Lang) -> &'static str {
    let names = match lang {
        Lang::De => &MONTHS_DE,
        Lang::En => &MONTHS_EN,
    };
    names[(month as usize).clamp(1, 12) - 1]
}

pub fn year_summary_csv(summary: &YearSummary, lang: Lang) -> String {
    let header = match lang {
        Lang::De => [
            "Monat",
            "Rechnungen",
            "Netto",
            "Steuer",
            "Brutto",
            "Zahlungseingänge",
        ],
        Lang::En => ["Month", "Invoices", "Net", "Tax", "Gross", "Receipts"],
    };
    let mut rows = vec![
        header
            .iter()
            .map(|h| text_cell(h, lang))
            .collect::<Vec<_>>(),
    ];
    let total_label = match lang {
        Lang::De => "Summe",
        Lang::En => "Total",
    };
    for m in summary.months.iter().chain(std::iter::once(&summary.total)) {
        let label = if m.month == 0 {
            total_label
        } else {
            month_name(m.month, lang)
        };
        rows.push(vec![
            text_cell(label, lang),
            m.count.to_string(),
            amount_cell(m.net, lang),
            amount_cell(m.tax, lang),
            amount_cell(m.gross, lang),
            amount_cell(m.received, lang),
        ]);
    }
    rows.push(Vec::new());
    let open_label = match lang {
        Lang::De => format!("Offen am {}", date_cell(summary.open_at, lang)),
        Lang::En => format!("Open on {}", date_cell(summary.open_at, lang)),
    };
    rows.push(vec![
        text_cell(&open_label, lang),
        amount_cell(summary.open_at_end, lang),
    ]);
    write_rows(&rows, lang)
}

/// `Rechnungen-2026-11-18.csv` or `Invoices-2026-11-18.csv`.
pub fn invoices_csv_file_name(today: NaiveDate, lang: Lang) -> String {
    let stem = match lang {
        Lang::De => "Rechnungen",
        Lang::En => "Invoices",
    };
    format!("{stem}-{}.csv", today.format("%Y-%m-%d"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::LineKind;
    use crate::invoice::{LineDraft, create_draft};
    use crate::masterdata::{NewCustomer, create_customer};
    use crate::tax::{TaxCategory, TaxChoice};
    use crate::testutil::{clock, d, sample_customer, sample_draft};
    use crate::year_summary::tests::{issue, pay, world};
    use rust_decimal::dec;

    fn customer_named(w: &mut crate::year_summary::tests::World, name: &str) {
        w.customer =
            w.db.write(|tx| {
                create_customer(
                    tx,
                    &clock(),
                    NewCustomer {
                        name: name.into(),
                        ..sample_customer()
                    },
                )
            })
            .unwrap();
    }

    fn csv(w: &crate::year_summary::tests::World, lang: Lang) -> String {
        invoices_csv(
            w.db.read(),
            &InvoiceFilter::default(),
            d(2026, 11, 18),
            lang,
        )
        .unwrap()
    }

    #[test]
    fn csv_quotes_separator_quotes_and_newlines() {
        let mut w = world();
        customer_named(&mut w, "Club \"A\"; Halle\nNord");
        issue(&mut w, d(2026, 11, 2), dec!(10));
        let out = csv(&w, Lang::De);
        assert_eq!(out.matches("\r\n").count(), 2, "{out:?}");
        assert!(out.contains(";\"Club \"\"A\"\"; Halle\nNord\";"), "{out:?}");
        assert_eq!(text_cell("a,b", Lang::En), "\"a,b\"");
        assert_eq!(text_cell("a,b", Lang::De), "a,b");
        assert_eq!(text_cell("a;b", Lang::De), "\"a;b\"");
    }

    #[test]
    fn csv_guards_formula_starts() {
        assert_eq!(text_cell("=SUM(A1)", Lang::De), "'=SUM(A1)");
        assert_eq!(text_cell("+49 123", Lang::De), "'+49 123");
        assert_eq!(text_cell("-x", Lang::De), "'-x");
        assert_eq!(text_cell("@a", Lang::De), "'@a");
        assert_eq!(text_cell("Kunde", Lang::De), "Kunde");
        assert_eq!(text_cell("\t=1", Lang::De), "'\t=1");
        assert_eq!(text_cell("\r=1", Lang::De), "\"'\r=1\"");
        let mut w = world();
        customer_named(&mut w, "=SUM(A1)");
        let a = issue(&mut w, d(2026, 11, 2), dec!(10));
        w.db.write(|tx| crate::storno::cancel_invoice(tx, &clock(), a))
            .unwrap();
        let out = csv(&w, Lang::De);
        assert!(out.contains(";'=SUM(A1);"), "{out:?}");
        assert!(out.contains(";-11,90;"), "{out:?}");
    }

    #[test]
    fn csv_numbers_and_dates_follow_language() {
        assert_eq!(amount_cell(Cents(123_456), Lang::De), "1234,56");
        assert_eq!(amount_cell(Cents(123_456), Lang::En), "1234.56");
        assert_eq!(amount_cell(Cents(-1_250), Lang::De), "-12,50");
        assert_eq!(amount_cell(Cents(5), Lang::En), "0.05");
        assert_eq!(date_cell(d(2026, 11, 18), Lang::De), "18.11.2026");
        assert_eq!(date_cell(d(2026, 11, 18), Lang::En), "2026-11-18");
        let mut w = world();
        issue(&mut w, d(2026, 11, 2), dec!(1000));
        let en = csv(&w, Lang::En);
        assert!(en.contains(",1190.00,EUR,"), "{en:?}");
        assert!(
            en.lines()
                .next()
                .unwrap()
                .contains("Number,Date,Service date")
        );
    }

    #[test]
    fn csv_has_bom_and_crlf() {
        let mut w = world();
        issue(&mut w, d(2026, 11, 2), dec!(10));
        let out = csv(&w, Lang::De);
        assert!(out.starts_with('\u{feff}'));
        assert!(out.ends_with("\r\n"));
        assert!(!out.replace("\r\n", "").contains('\n'));
        assert!(
            out.trim_start_matches('\u{feff}').starts_with(
                "Nummer;Datum;Leistungsdatum;Unternehmen;Marke;Kunde;Netto 19 %;Steuer 19 %;Brutto;Währung;Status;bezahlt;offen\r\n"
            ),
            "{out:?}"
        );
    }

    #[test]
    fn invoice_csv_columns_per_tax_rate() {
        let mut w = world();
        let a = issue(&mut w, d(2026, 11, 2), dec!(100));
        pay(&mut w, a, 1_000, d(2026, 11, 5));
        let mut input = sample_draft(w.brand, Some(w.customer));
        input.lines.push(LineDraft {
            kind: LineKind::Item,
            product_id: None,
            description: "Buch".into(),
            quantity: dec!(1),
            unit: "Stk".into(),
            unit_price: dec!(10),
            tax: TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(7) }),
        });
        w.db.write(|tx| create_draft(tx, &clock(), input)).unwrap();
        let out = csv(&w, Lang::De);
        let mut lines = out.trim_start_matches('\u{feff}').split("\r\n");
        let header = lines.next().unwrap();
        assert!(
            header.contains(";Kunde;Netto 19 %;Steuer 19 %;Netto 7 %;Steuer 7 %;Brutto;"),
            "{header}"
        );
        // The draft first (newest), then the issued invoice.
        let draft = lines.next().unwrap();
        assert!(draft.starts_with(";18.11.2026;18.11.2026;"), "{draft}");
        assert!(
            draft.contains(";25,00;4,75;10,00;0,70;40,45;EUR;Entwurf;;"),
            "{draft}"
        );
        let issued = lines.next().unwrap();
        assert!(
            issued.contains(";100,00;19,00;;;119,00;EUR;teilbezahlt, überfällig;10,00;109,00"),
            "{issued}"
        );
    }

    #[test]
    fn invoice_csv_uses_printed_customer_name() {
        let mut w = world();
        issue(&mut w, d(2026, 11, 2), dec!(10));
        let mut customer = crate::masterdata::get_customer(w.db.read(), w.customer).unwrap();
        customer.name = "Neuer Name".into();
        w.db.write(|tx| crate::masterdata::update_customer(tx, &clock(), &customer))
            .unwrap();
        let out = csv(&w, Lang::De);
        assert!(out.contains(";Kunde A;"), "{out:?}");
        assert!(!out.contains("Neuer Name"));
    }

    #[test]
    fn year_summary_csv_has_months_total_and_open() {
        let mut w = world();
        issue(&mut w, d(2025, 3, 4), dec!(100));
        let s = crate::year_summary::year_summary(w.db.read(), w.business, 2025, d(2026, 11, 18))
            .unwrap();
        let de = year_summary_csv(&s, Lang::De);
        let lines: Vec<&str> = de.trim_start_matches('\u{feff}').split("\r\n").collect();
        assert_eq!(
            lines[0],
            "Monat;Rechnungen;Netto;Steuer;Brutto;Zahlungseingänge"
        );
        assert_eq!(lines[3], "März;1;100,00;19,00;119,00;0,00");
        assert_eq!(lines[13], "Summe;1;100,00;19,00;119,00;0,00");
        assert_eq!(lines[14], "");
        assert_eq!(lines[15], "Offen am 31.12.2025;119,00");
        let en = year_summary_csv(&s, Lang::En);
        assert!(
            en.contains("\r\nMarch,1,100.00,19.00,119.00,0.00\r\n"),
            "{en:?}"
        );
        assert!(en.contains("\r\nOpen on 2025-12-31,119.00\r\n"), "{en:?}");
    }

    #[test]
    fn csv_file_names() {
        assert_eq!(
            invoices_csv_file_name(d(2026, 11, 18), Lang::De),
            "Rechnungen-2026-11-18.csv"
        );
        assert_eq!(
            invoices_csv_file_name(d(2026, 11, 18), Lang::En),
            "Invoices-2026-11-18.csv"
        );
    }
}
