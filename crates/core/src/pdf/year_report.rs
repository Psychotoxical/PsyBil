//! The year summary as a one-page PDF/A-3b document.

use chrono::Datelike;
use krilla::metadata::{DateTime, Metadata};

use crate::Result;
use crate::csv_export::month_name;
use crate::pdf::layout::{Align, Op, PageOps, Rgb};
use crate::pdf::locale::{Lang, date, money};
use crate::year_summary::{YearMonth, YearSummary};

const INK: Rgb = Rgb(33, 33, 33);
const MUTED: Rgb = Rgb(110, 110, 110);
const RULE: Rgb = Rgb(180, 180, 180);
const LEFT: f32 = 57.0;
const RIGHT: f32 = 538.0;
/// Right edges of the number columns: count, net, tax, gross, receipts.
const COLUMNS: [f32; 5] = [250.0, 330.0, 400.0, 470.0, RIGHT];

struct Texts {
    title: &'static str,
    amounts_in: &'static str,
    header: [&'static str; 6],
    total: &'static str,
    open_on: &'static str,
    created: &'static str,
    file_stem: &'static str,
}

fn texts(lang: Lang) -> Texts {
    match lang {
        Lang::De => Texts {
            title: "Jahresübersicht",
            amounts_in: "Beträge in",
            header: [
                "Monat",
                "Rechnungen",
                "Netto",
                "Steuer",
                "Brutto",
                "Eingänge",
            ],
            total: "Summe",
            open_on: "Offen am",
            created: "Erstellt am {date} mit PsyBil",
            file_stem: "Jahresübersicht",
        },
        Lang::En => Texts {
            title: "Year summary",
            amounts_in: "Amounts in",
            header: ["Month", "Invoices", "Net", "Tax", "Gross", "Receipts"],
            total: "Total",
            open_on: "Open on",
            created: "Created on {date} with PsyBil",
            file_stem: "Year-summary",
        },
    }
}

fn text(x: f32, y: f32, size: f32, bold: bool, color: Rgb, align: Align, text: String) -> Op {
    Op::Text {
        x,
        y,
        size,
        bold,
        tabular: align == Align::Right,
        color,
        align,
        text,
    }
}

fn rule(y: f32) -> Op {
    Op::Line {
        x1: LEFT,
        y1: y,
        x2: RIGHT,
        y2: y,
        width: 0.5,
        color: RULE,
    }
}

fn row(ops: &mut Vec<Op>, y: f32, label: &str, m: &YearMonth, currency: &str, lang: Lang) {
    let bold = m.month == 0;
    ops.push(text(
        LEFT,
        y,
        9.5,
        bold,
        INK,
        Align::Left,
        label.to_string(),
    ));
    let cells = [
        m.count.to_string(),
        money(m.net, currency, lang),
        money(m.tax, currency, lang),
        money(m.gross, currency, lang),
        money(m.received, currency, lang),
    ];
    for (x, cell) in COLUMNS.iter().zip(cells) {
        ops.push(text(*x, y, 9.5, bold, INK, Align::Right, cell));
    }
}

/// The PDF of `summary` in `lang`.
pub fn year_report_pdf(summary: &YearSummary, lang: Lang) -> Result<Vec<u8>> {
    let t = texts(lang);
    let currency = summary.currency.as_str();
    let title = format!("{} {}", t.title, summary.year);
    let mut ops = vec![
        text(LEFT, 80.0, 18.0, true, INK, Align::Left, title.clone()),
        text(
            LEFT,
            100.0,
            10.0,
            false,
            MUTED,
            Align::Left,
            format!("{} · {} {currency}", summary.business_name, t.amounts_in),
        ),
    ];
    let header_y = 140.0;
    ops.push(text(
        LEFT,
        header_y,
        9.0,
        true,
        MUTED,
        Align::Left,
        t.header[0].into(),
    ));
    for (x, label) in COLUMNS.iter().zip(&t.header[1..]) {
        ops.push(text(
            *x,
            header_y,
            9.0,
            true,
            MUTED,
            Align::Right,
            (*label).into(),
        ));
    }
    ops.push(rule(header_y + 6.0));
    let mut y = header_y + 24.0;
    for m in &summary.months {
        row(&mut ops, y, month_name(m.month, lang), m, currency, lang);
        y += 18.0;
    }
    ops.push(rule(y - 10.0));
    y += 6.0;
    row(&mut ops, y, t.total, &summary.total, currency, lang);
    y += 40.0;
    ops.push(text(
        LEFT,
        y,
        11.0,
        true,
        INK,
        Align::Left,
        format!(
            "{} {}: {}",
            t.open_on,
            date(summary.open_at, lang),
            money(summary.open_at_end, currency, lang)
        ),
    ));
    ops.push(text(
        LEFT,
        800.0,
        8.0,
        false,
        MUTED,
        Align::Left,
        crate::pdf::locale::fill(t.created, &[("date", &date(summary.created_on, lang))]),
    ));
    let created = summary.created_on;
    let metadata = Metadata::new()
        .title(format!("{title} · {}", summary.business_name))
        .authors(vec![summary.business_name.clone()])
        .language(
            match lang {
                Lang::De => "de-DE",
                Lang::En => "en-GB",
            }
            .into(),
        )
        .creator("PsyBil".into())
        .creation_date(
            DateTime::new(created.year() as u16)
                .month(created.month() as u8)
                .day(created.day() as u8),
        );
    crate::pdf::render::write_pages(&[PageOps { ops }], metadata, None)
}

/// `Jahresübersicht-2026-Beispiel_GbR.pdf` or `Year-summary-2026-Beispiel_GbR.pdf`.
pub fn year_report_file_name(summary: &YearSummary, lang: Lang, ext: &str) -> String {
    let raw = format!(
        "{}-{}-{}",
        texts(lang).file_stem,
        summary.year,
        summary.business_name
    );
    format!("{}.{ext}", crate::pdf::export::safe_stem(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::d;
    use crate::year_summary::tests::{issue, pay, world};
    use crate::year_summary::year_summary;
    use rust_decimal::dec;

    #[test]
    fn year_report_pdf_text_has_months_and_totals() {
        let mut w = world();
        let a = issue(&mut w, d(2026, 3, 4), dec!(100));
        pay(&mut w, a, 5_000, d(2026, 3, 10));
        let s = year_summary(w.db.read(), w.business, 2026, d(2026, 11, 18)).unwrap();
        let pdf = year_report_pdf(&s, Lang::De).unwrap();
        let text = crate::pdf::render::tests::text_of(&pdf);
        for needle in [
            "Jahresübersicht 2026",
            "Beispiel GbR",
            "Januar",
            "März",
            "Dezember",
            "Summe",
            "119,00 €",
            "50,00 €",
            "Offen am 18.11.2026",
            "69,00 €",
            "Erstellt am 18.11.2026",
        ] {
            assert!(text.contains(needle), "{needle} missing in {text:?}");
        }
        let en = crate::pdf::render::tests::text_of(&year_report_pdf(&s, Lang::En).unwrap());
        assert!(en.contains("Year summary 2026"), "{en:?}");
        assert!(en.contains("€119.00"), "{en:?}");
    }

    /// Writes a sample year report to `PSYBIL_SAMPLES`, for looking at it.
    #[test]
    #[ignore]
    fn write_year_report_sample() {
        let dir = std::path::PathBuf::from(std::env::var("PSYBIL_SAMPLES").unwrap());
        std::fs::create_dir_all(&dir).unwrap();
        let mut w = world();
        for (month, net) in [
            (1, dec!(450)),
            (3, dec!(1200)),
            (6, dec!(80)),
            (9, dec!(2300)),
        ] {
            let a = issue(&mut w, d(2026, month, 4), net);
            if month < 9 {
                pay(
                    &mut w,
                    a,
                    (net * dec!(119)).try_into().unwrap(),
                    d(2026, month, 20),
                );
            }
        }
        let s = year_summary(w.db.read(), w.business, 2026, d(2026, 11, 18)).unwrap();
        std::fs::write(
            dir.join("jahresuebersicht.pdf"),
            year_report_pdf(&s, Lang::De).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn year_report_file_names() {
        let w = world();
        let s = year_summary(w.db.read(), w.business, 2026, d(2026, 11, 18)).unwrap();
        assert_eq!(
            year_report_file_name(&s, Lang::De, "pdf"),
            "Jahresübersicht-2026-Beispiel_GbR.pdf"
        );
        assert_eq!(
            year_report_file_name(&s, Lang::En, "csv"),
            "Year-summary-2026-Beispiel_GbR.csv"
        );
    }
}
