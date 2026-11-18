//! Places an invoice on A4 pages as drawing commands in points (origin top
//! left, y down, text `y` is the baseline). The positions follow DIN 5008
//! form B, so the address shows in a window envelope.

use crate::calc::PriceMode;
use crate::dates::ServiceDate;
use crate::invoice::InvoiceKind;
use crate::money::Cents;
use crate::pdf::fonts::{Fonts, PdfFont, text_width, wrap};
use crate::pdf::locale::{Labels, Lang, date, fill, money, price, quantity, rate, tax_note};
use crate::pdf::mm;
use crate::pdf::model::InvoiceDoc;
use crate::tax::{TaxCategory, TaxChoice};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Template {
    Classic,
    Modern,
    Minimal,
}

impl Template {
    pub(crate) fn from_name(name: &str) -> Template {
        match name.trim().to_lowercase().as_str() {
            "modern" => Template::Modern,
            "minimal" => Template::Minimal,
            _ => Template::Classic,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rgb(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    /// `x` is the right edge.
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Op {
    Text {
        x: f32,
        y: f32,
        size: f32,
        bold: bool,
        tabular: bool,
        color: Rgb,
        align: Align,
        text: String,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: Rgb,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
        color: Rgb,
    },
    Logo {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Watermark {
        text: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PageOps {
    pub ops: Vec<Op>,
}

const INK: Rgb = Rgb(33, 33, 33);
const MUTED: Rgb = Rgb(110, 110, 110);
const RULE: Rgb = Rgb(180, 180, 180);
const MARK: Rgb = Rgb(150, 150, 150);
const HEAD_FILL: Rgb = Rgb(245, 245, 245);
const ZEBRA: Rgb = Rgb(250, 250, 250);
const HAIRLINE: Rgb = Rgb(220, 220, 220);

/// `#rrggbb` or `#rgb`.
fn parse_color(text: &str) -> Option<Rgb> {
    let hex = text.trim().strip_prefix('#')?;
    let digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect::<Option<_>>()?;
    match digits.as_slice() {
        [r, g, b] => Some(Rgb(r * 17, g * 17, b * 17)),
        [r1, r2, g1, g2, b1, b2] => Some(Rgb(r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2)),
        _ => None,
    }
}

/// Relative luminance after WCAG 2.
fn luminance(c: Rgb) -> f32 {
    let channel = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(c.0) + 0.7152 * channel(c.1) + 0.0722 * channel(c.2)
}

/// Contrast ratio of two colours after WCAG 2.
pub(crate) fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Text colour on a filled background: white or ink, whichever stands out
/// more.
fn on_color(background: Rgb) -> Rgb {
    let white = Rgb(255, 255, 255);
    if contrast(white, background) >= contrast(INK, background) {
        white
    } else {
        INK
    }
}

/// The colour itself when it reads well on white paper (4.5 : 1), else
/// darkened until it does.
fn readable_on_white(color: Rgb) -> Rgb {
    let white = Rgb(255, 255, 255);
    let mut c = color;
    let mut step = 0;
    while contrast(c, white) < 4.5 && step < 20 {
        step += 1;
        let k = 1.0 - step as f32 * 0.05;
        c = Rgb(
            (color.0 as f32 * k) as u8,
            (color.1 as f32 * k) as u8,
            (color.2 as f32 * k) as u8,
        );
    }
    c
}

// Positions in millimetres.
const LEFT: f32 = 25.0;
const RIGHT: f32 = 190.0;
const BODY_LIMIT: f32 = 262.0;
const INFO_LEFT: f32 = 120.0;
/// Filled areas reach this far past the text edges.
const PAD: f32 = 1.5;
const BODY_SIZE: f32 = 9.0;
const BODY_LEAD: f32 = 4.2;
const ROW_GAP: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Col {
    Pos,
    Desc,
    Qty,
    Unit,
    Vat,
    UnitPrice,
    Amount,
}

/// Table columns: what, left edge, right edge, alignment (mm).
fn columns(small: bool) -> Vec<(Col, f32, f32, Align)> {
    use Align::{Left, Right};
    if small {
        vec![
            (Col::Pos, 25.0, 33.0, Left),
            (Col::Desc, 33.0, 115.0, Left),
            (Col::Qty, 115.0, 133.0, Right),
            (Col::Unit, 135.0, 148.0, Left),
            (Col::UnitPrice, 148.0, 170.0, Right),
            (Col::Amount, 170.0, 190.0, Right),
        ]
    } else {
        vec![
            (Col::Pos, 25.0, 33.0, Left),
            (Col::Desc, 33.0, 103.0, Left),
            (Col::Qty, 103.0, 121.0, Right),
            (Col::Unit, 123.0, 136.0, Left),
            (Col::Vat, 136.0, 148.0, Right),
            (Col::UnitPrice, 148.0, 170.0, Right),
            (Col::Amount, 170.0, 190.0, Right),
        ]
    }
}

/// Whether the invoice is one of a small business: no VAT anywhere.
fn is_small(doc: &InvoiceDoc) -> bool {
    if doc.totals.groups.is_empty() {
        return doc.business.small_business;
    }
    doc.totals
        .groups
        .iter()
        .all(|g| g.category == TaxCategory::SmallBusiness)
}

struct Builder<'a> {
    doc: &'a InvoiceDoc,
    fonts: &'static Fonts,
    labels: &'static Labels,
    lang: Lang,
    template: Template,
    small: bool,
    /// The brand colour (grey when unreadable).
    accent: Rgb,
    pages: Vec<PageOps>,
    /// Top of the free space on the current page, in mm.
    y: f32,
    /// Where the body starts on the current page.
    page_top: f32,
    /// A new page repeats the table head while rows are being placed.
    in_table: bool,
}

impl<'a> Builder<'a> {
    fn new(doc: &'a InvoiceDoc) -> Builder<'a> {
        let lang = Lang::from_code(&doc.language);
        Builder {
            doc,
            fonts: Fonts::get(),
            labels: Labels::of(lang),
            lang,
            template: Template::from_name(&doc.template),
            small: is_small(doc),
            accent: parse_color(&doc.brand.color).unwrap_or(Rgb(90, 90, 90)),
            pages: vec![PageOps::default()],
            y: 0.0,
            page_top: 0.0,
            in_table: false,
        }
    }

    /// Title and total: the brand colour in the modern template.
    fn highlight(&self) -> Rgb {
        if self.template == Template::Modern {
            readable_on_white(self.accent)
        } else {
            INK
        }
    }

    fn brand_name(&self) -> String {
        if self.doc.brand.name.trim().is_empty() {
            self.doc.business.name.clone()
        } else {
            self.doc.brand.name.clone()
        }
    }

    /// Starts a follow-up page with a short head.
    fn new_page(&mut self) {
        self.pages.push(PageOps::default());
        if self.doc.number.is_none() {
            self.push(Op::Watermark {
                text: self.labels.draft_mark.to_string(),
            });
        }
        let (brand, size) = self.fit(&self.brand_name(), 9.0, true, false, 80.0);
        self.text(LEFT, 15.0, size, true, INK, &brand);
        let head = format!("{} {}", self.title(), self.number_text());
        let (head, size) = self.fit(&head, 9.0, false, false, 80.0);
        self.right(RIGHT, 15.0, size, false, MUTED, &head);
        self.y = 25.0;
        self.page_top = 25.0;
        if self.in_table {
            self.table_header();
        }
    }

    fn push(&mut self, op: Op) {
        self.pages.last_mut().expect("a page").ops.push(op);
    }

    fn font(&self, bold: bool) -> &'static PdfFont {
        if bold {
            &self.fonts.bold
        } else {
            &self.fonts.regular
        }
    }

    /// Width of a text in mm.
    fn width(&self, text: &str, size: f32, bold: bool) -> f32 {
        text_width(self.font(bold), text, size, false) / mm(1.0)
    }

    /// `text` at the largest size up to `size` that fits `max_mm`; below
    /// 6 pt it is cut short with an ellipsis instead.
    fn fit(&self, text: &str, size: f32, bold: bool, tabular: bool, max_mm: f32) -> (String, f32) {
        const MIN: f32 = 6.0;
        let font = self.font(bold);
        let width = |t: &str, s: f32| text_width(font, t, s, tabular) / mm(1.0);
        let full = width(text, size);
        if full <= max_mm {
            return (text.to_string(), size);
        }
        let scaled = size * max_mm / full * 0.995;
        if scaled >= MIN {
            return (text.to_string(), scaled);
        }
        let mut chars: Vec<char> = text.chars().collect();
        while chars.pop().is_some() {
            let cut = format!("{}…", chars.iter().collect::<String>().trim_end());
            if width(&cut, MIN) <= max_mm {
                return (cut, MIN);
            }
        }
        ("…".to_string(), MIN)
    }

    fn wrap(&self, text: &str, size: f32, bold: bool, width_mm: f32) -> Vec<String> {
        wrap(self.font(bold), text, size, mm(width_mm))
    }

    #[allow(clippy::too_many_arguments)]
    fn text_with(
        &mut self,
        x: f32,
        y: f32,
        size: f32,
        bold: bool,
        tabular: bool,
        color: Rgb,
        align: Align,
        text: &str,
    ) {
        if text.is_empty() {
            return;
        }
        self.push(Op::Text {
            x: mm(x),
            y: mm(y),
            size,
            bold,
            tabular,
            color,
            align,
            text: text.to_string(),
        });
    }

    fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, color: Rgb, text: &str) {
        self.text_with(x, y, size, bold, false, color, Align::Left, text);
    }

    fn right(&mut self, x: f32, y: f32, size: f32, bold: bool, color: Rgb, text: &str) {
        self.text_with(x, y, size, bold, false, color, Align::Right, text);
    }

    /// A right-aligned amount with digits of equal width.
    fn amount(&mut self, x: f32, y: f32, size: f32, bold: bool, color: Rgb, text: &str) {
        self.text_with(x, y, size, bold, true, color, Align::Right, text);
    }

    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Rgb) {
        self.push(Op::Line {
            x1: mm(x1),
            y1: mm(y1),
            x2: mm(x2),
            y2: mm(y2),
            width,
            color,
        });
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, fill: Rgb) {
        self.push(Op::Rect {
            x: mm(x),
            y: mm(y),
            w: mm(w),
            h: mm(h),
            fill,
        });
    }

    fn money(&self, amount: Cents) -> String {
        money(amount, &self.doc.currency, self.lang)
    }

    fn number_text(&self) -> String {
        self.doc
            .number
            .clone()
            .unwrap_or_else(|| self.labels.draft_number.to_string())
    }

    fn title(&self) -> &'static str {
        match self.doc.kind {
            InvoiceKind::Invoice => self.labels.title_invoice,
            InvoiceKind::Storno => self.labels.title_storno,
        }
    }

    /// Makes room for `height` mm; a new page when the body is full.
    fn ensure(&mut self, height: f32) {
        if self.y + height > BODY_LIMIT && self.y > self.page_top + 0.01 {
            self.new_page();
        }
    }

    /// A wrapped paragraph over the full width.
    fn paragraph(&mut self, text: &str, size: f32, color: Rgb) {
        let lead = size * 0.47;
        for line in self.wrap(text, size, false, RIGHT - LEFT) {
            self.ensure(lead);
            self.text(LEFT, self.y + lead * 0.75, size, false, color, &line);
            self.y += lead;
        }
    }

    // ------------------------------------------------------------- first page

    fn first_page(&mut self, logo_px: Option<(u32, u32)>) {
        if self.doc.number.is_none() {
            self.push(Op::Watermark {
                text: self.labels.draft_mark.to_string(),
            });
        }
        if self.template == Template::Modern {
            self.rect(0.0, 0.0, 210.0, 6.0, self.accent);
        }
        self.marks();
        let mut name_room = RIGHT - LEFT;
        if let Some((w, h)) = logo_px.filter(|(w, h)| *w > 0 && *h > 0) {
            let scale = (50.0 / w as f32).min(25.0 / h as f32);
            let (lw, lh) = (w as f32 * scale, h as f32 * scale);
            self.push(Op::Logo {
                x: mm(RIGHT - lw),
                y: mm(12.0),
                w: mm(lw),
                h: mm(lh),
            });
            name_room = RIGHT - lw - 5.0 - LEFT;
        }
        let (brand_name, size) = self.fit(&self.brand_name(), 14.0, true, false, name_room);
        self.text(LEFT, 22.0, size, true, INK, &brand_name);
        self.address();
        self.info_block();
        let title = self.title();
        let title_color = self.highlight();
        self.text(LEFT, 100.0, 16.0, true, title_color, title);
        self.y = 104.0;
        self.page_top = 104.0;
        if self.doc.imported {
            let note = self.labels.imported;
            self.text(LEFT, self.y + 3.0, 8.0, false, MUTED, note);
            self.y += 5.0;
        }
        self.y += 2.0;
        if !self.doc.intro.trim().is_empty() {
            let intro = self.doc.intro.clone();
            self.paragraph(&intro, BODY_SIZE, INK);
            self.y += 3.0;
        }
    }

    /// Fold and punch marks at the left edge.
    fn marks(&mut self) {
        self.line(0.0, 105.0, 5.0, 105.0, 0.25, MARK);
        self.line(0.0, 210.0, 5.0, 210.0, 0.25, MARK);
        self.line(0.0, 148.5, 7.0, 148.5, 0.25, MARK);
    }

    fn address(&mut self) {
        let b = &self.doc.business;
        let city = format!("{} {}", b.zip, b.city).trim().to_string();
        let sender = [b.name.as_str(), b.street.as_str(), city.as_str()]
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        if let Some(first) = self.wrap(&sender, 7.0, false, 80.0).into_iter().next() {
            self.text(LEFT, 50.0, 7.0, false, MUTED, &first);
        }
        let Some(c) = &self.doc.customer else {
            return;
        };
        let city = format!("{} {}", c.zip, c.city).trim().to_string();
        let country = c.country.trim().to_uppercase();
        let mut lines: Vec<String> = vec![c.name.clone(), c.street.clone(), city];
        if !country.is_empty() && country != "DE" {
            lines.push(country);
        }
        let lines: Vec<String> = lines
            .into_iter()
            .filter(|l| !l.trim().is_empty())
            .flat_map(|l| self.wrap(&l, 10.0, false, 80.0))
            .take(6)
            .collect();
        for (i, line) in lines.iter().enumerate() {
            self.text(LEFT, 63.0 + i as f32 * 4.5, 10.0, false, INK, line);
        }
    }

    fn needs_vat_ids(&self) -> bool {
        self.doc.totals.groups.iter().any(|g| {
            matches!(
                g.category,
                TaxCategory::ReverseCharge | TaxCategory::IntraCommunity
            )
        })
    }

    fn info_block(&mut self) {
        let (l, lang) = (self.labels, self.lang);
        let doc = self.doc;
        let mut rows: Vec<(&str, String)> = vec![
            (l.number, self.number_text()),
            (l.invoice_date, date(doc.invoice_date, lang)),
        ];
        match &doc.service_date {
            ServiceDate::SameAsInvoice => {
                rows.push((l.service_date, l.same_as_invoice.to_string()))
            }
            ServiceDate::Day { date: day } => rows.push((l.service_date, date(*day, lang))),
            ServiceDate::Period { from, to } => rows.push((
                l.service_period,
                format!("{} – {}", date(*from, lang), date(*to, lang)),
            )),
            ServiceDate::None => {}
        }
        if doc.kind != InvoiceKind::Storno {
            rows.push((l.due, date(doc.due_date, lang)));
        }
        if let Some((number, day)) = &doc.cancels {
            rows.push((
                l.cancels,
                fill(
                    l.cancels_value,
                    &[("number", number), ("date", &date(*day, lang))],
                ),
            ));
        }
        if self.needs_vat_ids()
            && let Some(c) = &doc.customer
            && !c.vat_id.trim().is_empty()
        {
            rows.push((l.customer_vat, c.vat_id.clone()));
        }
        let mut y = 50.0;
        for (label, value) in &rows {
            self.text(INFO_LEFT, y, BODY_SIZE, false, MUTED, label);
            // A value that does not fit next to its label goes below it.
            let room = RIGHT - INFO_LEFT - self.width(label, BODY_SIZE, false) - 3.0;
            if self.width(value, BODY_SIZE, false) > room {
                y += 4.5;
            }
            let (value, size) = self.fit(value, BODY_SIZE, false, false, RIGHT - INFO_LEFT);
            self.right(RIGHT, y, size, false, INK, &value);
            y += 4.5;
        }
    }

    // ------------------------------------------------------------------ table

    fn table_header(&mut self) {
        let height = if self.small { 5.0 } else { 8.5 };
        self.ensure(height + BODY_LEAD + ROW_GAP + 1.0);
        let (head_ink, head_muted) = match self.template {
            Template::Classic => {
                self.rect(
                    LEFT - PAD,
                    self.y,
                    RIGHT - LEFT + 2.0 * PAD,
                    height,
                    HEAD_FILL,
                );
                (INK, MUTED)
            }
            Template::Modern => {
                self.rect(
                    LEFT - PAD,
                    self.y,
                    RIGHT - LEFT + 2.0 * PAD,
                    height,
                    self.accent,
                );
                let ink = on_color(self.accent);
                (ink, ink)
            }
            Template::Minimal => {
                let y = self.y + height;
                self.line(LEFT, y, RIGHT, y, 0.5, INK);
                (INK, MUTED)
            }
        };
        let l = self.labels;
        let kind = match self.doc.price_mode {
            PriceMode::Net => l.net,
            PriceMode::Gross => l.gross,
        };
        let top = self.y;
        for (col, left, right, align) in columns(self.small) {
            let label = match col {
                Col::Pos => l.col_pos,
                Col::Desc => l.col_desc,
                Col::Qty => l.col_qty,
                Col::Unit => l.col_unit,
                Col::Vat => l.col_vat,
                Col::UnitPrice => l.col_unit_price,
                Col::Amount => l.col_amount,
            };
            let x = match align {
                Align::Left => left + 1.0,
                Align::Right => right,
            };
            self.text_with(x, top + 3.5, 8.0, true, false, head_ink, align, label);
            if !self.small && matches!(col, Col::UnitPrice | Col::Amount) {
                self.text_with(x, top + 7.0, 7.0, false, false, head_muted, align, kind);
            }
        }
        self.y += height + 1.0;
    }

    fn vat_text(&self, tax: &TaxChoice) -> String {
        match tax {
            TaxChoice::Fixed(category) => rate(category.rate(), self.lang),
            TaxChoice::FollowGoods => String::new(),
        }
    }

    fn rows(&mut self) {
        let doc = self.doc;
        let desc_width = columns(self.small)
            .iter()
            .find(|c| c.0 == Col::Desc)
            .map(|c| c.2 - c.1 - 2.0)
            .unwrap_or(60.0);
        for (index, line) in doc.lines.iter().enumerate() {
            let mut desc = self.wrap(&line.description, BODY_SIZE, false, desc_width);
            if desc.is_empty() {
                desc.push(String::new());
            }
            let height = desc.len() as f32 * BODY_LEAD + ROW_GAP;
            let head = if self.small { 6.0 } else { 9.5 };
            // Only a position taller than a whole page is split, line by line.
            let split = height > BODY_LIMIT - 25.0 - head;
            self.ensure(if split { BODY_LEAD + ROW_GAP } else { height });
            if !split && self.template == Template::Classic && index % 2 == 1 {
                self.rect(LEFT - PAD, self.y, RIGHT - LEFT + 2.0 * PAD, height, ZEBRA);
            }
            let base = self.y + 3.2;
            let amount = doc
                .totals
                .line_amounts
                .get(index)
                .map(|a| self.money(*a))
                .unwrap_or_default();
            for (col, left, right, align) in columns(self.small) {
                let value = match col {
                    Col::Pos => (index + 1).to_string(),
                    Col::Desc => continue,
                    Col::Qty => quantity(line.quantity, self.lang),
                    Col::Unit => line.unit.clone(),
                    Col::Vat => self.vat_text(&line.tax),
                    Col::UnitPrice => price(line.unit_price, &doc.currency, self.lang),
                    Col::Amount => amount.clone(),
                };
                let x = match align {
                    Align::Left => left + 1.0,
                    Align::Right => right,
                };
                let tabular = align == Align::Right;
                let (value, size) = self.fit(&value, BODY_SIZE, false, tabular, right - left - 1.0);
                self.text_with(x, base, size, false, tabular, INK, align, &value);
            }
            if split {
                for (i, text) in desc.iter().enumerate() {
                    if i > 0 {
                        self.ensure(BODY_LEAD);
                    }
                    self.text(34.0, self.y + 3.2, BODY_SIZE, false, INK, text);
                    self.y += BODY_LEAD;
                }
                self.y += ROW_GAP;
                continue;
            }
            for (i, text) in desc.iter().enumerate() {
                self.text(
                    34.0,
                    base + i as f32 * BODY_LEAD,
                    BODY_SIZE,
                    false,
                    INK,
                    text,
                );
            }
            self.y += height;
            if self.template == Template::Minimal {
                self.line(LEFT, self.y, RIGHT, self.y, 0.25, HAIRLINE);
            }
        }
        self.in_table = false;
    }

    // ----------------------------------------------------------------- totals

    fn total_rows(&self) -> Vec<(String, String, bool)> {
        let (doc, l) = (self.doc, self.labels);
        let total = (l.total.to_string(), self.money(doc.totals.gross), true);
        if self.small {
            return vec![total];
        }
        let group = |template: &str, g: &crate::calc::TaxGroup| {
            (
                fill(
                    template,
                    &[
                        ("rate", &rate(g.category.rate(), self.lang)),
                        ("base", &self.money(g.base)),
                    ],
                ),
                self.money(g.tax),
                false,
            )
        };
        match doc.price_mode {
            PriceMode::Net => {
                let mut rows = vec![(
                    l.subtotal_net.to_string(),
                    self.money(doc.totals.net),
                    false,
                )];
                rows.extend(doc.totals.groups.iter().map(|g| group(l.plus_vat, g)));
                rows.push(total);
                rows
            }
            PriceMode::Gross => {
                let mut rows = vec![total];
                rows.extend(doc.totals.groups.iter().map(|g| group(l.incl_vat, g)));
                rows
            }
        }
    }

    /// The texts under the totals: tax notes, payment request, closing text.
    fn closing_texts(&self) -> Vec<String> {
        let (doc, l) = (self.doc, self.labels);
        let mut texts: Vec<String> = Vec::new();
        let categories: Vec<TaxCategory> = if doc.totals.groups.is_empty() && self.small {
            vec![TaxCategory::SmallBusiness]
        } else {
            doc.totals
                .groups
                .iter()
                .map(|g| g.category.clone())
                .collect()
        };
        for category in categories {
            if let Some(note) = tax_note(&category, self.lang)
                && !note.trim().is_empty()
                && !texts.contains(&note)
            {
                texts.push(note);
            }
        }
        if doc.kind != InvoiceKind::Storno {
            let due = date(doc.due_date, self.lang);
            let pay = if doc.payment_received {
                l.paid.to_string()
            } else if doc.brand.iban.trim().is_empty() {
                fill(l.pay_by, &[("due", &due)])
            } else {
                fill(
                    l.pay_transfer,
                    &[
                        ("amount", &self.money(doc.totals.gross)),
                        ("due", &due),
                        ("number", &self.number_text()),
                    ],
                )
            };
            texts.push(pay);
        }
        if !doc.outro.trim().is_empty() {
            texts.push(doc.outro.clone());
        }
        texts
    }

    fn totals(&mut self) {
        let rows = self.total_rows();
        let texts = self.closing_texts();
        let text_height: f32 = texts
            .iter()
            .map(|t| self.wrap(t, BODY_SIZE, false, RIGHT - LEFT).len() as f32 * BODY_LEAD + 2.0)
            .sum();
        self.ensure(4.0 + rows.len() as f32 * 5.0 + 4.0 + text_height);
        self.y += 4.0;
        for (label, value, bold) in rows {
            if bold {
                self.line(120.0, self.y, RIGHT, self.y, 0.5, RULE);
            }
            let size = if bold { 10.0 } else { BODY_SIZE };
            let base = self.y + 3.75;
            let ink = if bold { self.highlight() } else { INK };
            // The amount first, the label gets what is left.
            let (value, value_size) = self.fit(&value, size, bold, true, 30.0);
            let value_width = text_width(self.font(bold), &value, value_size, true) / mm(1.0);
            let (label, label_size) =
                self.fit(&label, size, bold, false, RIGHT - 120.0 - value_width - 3.0);
            self.text(120.0, base, label_size, bold, ink, &label);
            self.amount(RIGHT, base, value_size, bold, ink, &value);
            self.y += 5.0;
        }
        self.y += 4.0;
        for text in texts {
            self.paragraph(&text, BODY_SIZE, INK);
            self.y += 2.0;
        }
    }

    // ----------------------------------------------------------------- footer

    fn footer_columns(&self) -> Vec<(f32, f32, Vec<String>)> {
        let (b, brand, l) = (&self.doc.business, &self.doc.brand, self.labels);
        let labelled = |label: &str, value: &str| {
            if value.trim().is_empty() {
                String::new()
            } else {
                format!("{label} {value}")
            }
        };
        vec![
            (
                25.0,
                68.0,
                vec![
                    b.name.clone(),
                    b.street.clone(),
                    format!("{} {}", b.zip, b.city).trim().to_string(),
                ],
            ),
            (
                68.0,
                106.0,
                vec![
                    labelled(l.phone, &brand.phone),
                    labelled(l.email, &brand.email),
                    labelled(l.web, &brand.website),
                ],
            ),
            (
                106.0,
                152.0,
                vec![
                    brand.bank_name.clone(),
                    labelled(l.iban, &brand.iban),
                    labelled(l.bic, &brand.bic),
                ],
            ),
            (
                152.0,
                190.0,
                vec![
                    labelled(l.tax_number, &b.tax_number),
                    labelled(l.vat_id, &b.vat_id),
                ],
            ),
        ]
    }

    fn footers(&mut self) {
        let count = self.pages.len();
        let columns = self.footer_columns();
        for index in 0..count {
            let mut footer = Builder {
                pages: vec![PageOps::default()],
                ..Builder::new(self.doc)
            };
            footer.line(LEFT, 270.0, RIGHT, 270.0, 0.25, RULE);
            let label = fill(
                self.labels.page,
                &[("x", &(index + 1).to_string()), ("y", &count.to_string())],
            );
            footer.right(RIGHT, 266.0, 8.0, false, MUTED, &label);
            for (left, right, entries) in &columns {
                let lines: Vec<String> = entries
                    .iter()
                    .filter(|e| !e.trim().is_empty())
                    .flat_map(|e| footer.wrap(e, 7.0, false, right - left - 2.0))
                    .collect();
                for (i, line) in lines.iter().enumerate() {
                    footer.text(*left, 274.0 + i as f32 * 3.2, 7.0, false, MUTED, line);
                }
            }
            let ops = footer.pages.remove(0).ops;
            self.pages[index].ops.extend(ops);
        }
    }
}

/// Lays out `doc`; `logo_px` is the pixel size of the logo, if one is printed.
pub(crate) fn layout(doc: &InvoiceDoc, logo_px: Option<(u32, u32)>) -> Vec<PageOps> {
    let mut b = Builder::new(doc);
    b.first_page(logo_px);
    b.in_table = true;
    b.table_header();
    b.rows();
    b.totals();
    b.footers();
    b.pages
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::Db;
    use crate::invoice::create_draft;
    use crate::masterdata::{NewBusiness, NewCustomer, create_business, create_customer};
    use crate::pdf::fonts::{Fonts, text_width};
    use crate::pdf::mm;
    use crate::pdf::model::draft_doc;
    use crate::tax::{TaxCategory, TaxChoice};
    use crate::testutil::{clock, sample_business, sample_customer, sample_draft};

    /// A draft document; `adjust` changes business, customer and draft first.
    pub(crate) fn doc_with(
        adjust: impl FnOnce(&mut NewBusiness, &mut Option<NewCustomer>, &mut crate::invoice::DraftInput),
    ) -> InvoiceDoc {
        let mut db = Db::open_in_memory().unwrap();
        let mut business = sample_business();
        business.vat_id = String::new();
        let mut customer = Some(sample_customer());
        let mut input = sample_draft(uuid::Uuid::nil(), None);
        adjust(&mut business, &mut customer, &mut input);
        let (_, brand) = db
            .write(|tx| create_business(tx, &clock(), business))
            .unwrap();
        let customer_id =
            customer.map(|c| db.write(|tx| create_customer(tx, &clock(), c)).unwrap());
        input.brand_id = brand;
        input.customer_id = customer_id;
        let id = db.write(|tx| create_draft(tx, &clock(), input)).unwrap();
        draft_doc(db.read(), id).unwrap()
    }

    pub(crate) fn sample() -> InvoiceDoc {
        doc_with(|_, _, _| {})
    }

    pub(crate) fn texts(pages: &[PageOps]) -> Vec<(f32, f32, Align, String)> {
        pages
            .iter()
            .flat_map(|p| &p.ops)
            .filter_map(|op| match op {
                Op::Text {
                    x, y, align, text, ..
                } => Some((*x, *y, *align, text.clone())),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn joined(pages: &[PageOps]) -> String {
        texts(pages)
            .into_iter()
            .map(|t| t.3)
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn width(text: &str, size: f32) -> f32 {
        text_width(&Fonts::get().regular, text, size, false)
    }

    #[test]
    fn recipient_sits_in_window() {
        let pages = layout(&sample(), None);
        let mut found = 0;
        for op in pages.iter().flat_map(|p| &p.ops) {
            if let Op::Text {
                x, y, size, text, ..
            } = op
                && ["Kunde A", "Hauptstraße 2", "54321 Beispielstadt"].contains(&text.as_str())
            {
                found += 1;
                assert!(
                    *x >= mm(20.0) && x + width(text, *size) <= mm(105.0),
                    "{text}"
                );
                assert!(*y > mm(45.0) && *y < mm(90.0), "{text}");
            }
        }
        assert_eq!(found, 3);
    }

    #[test]
    fn mandatory_fields_present() {
        let text = joined(&layout(&sample(), None));
        for needed in [
            "Beispiel GbR",
            "Musterstraße 1",
            "12345 Musterstadt",
            "12/345/67890",
            "Kunde A",
            "Hauptstraße 2",
            "54321 Beispielstadt",
            "18.11.2026",
            "entspricht Rechnungsdatum",
            "Leistung",
            "2",
            "25,00 €",
            "19 %",
            "4,75 €",
            "29,75 €",
        ] {
            assert!(text.contains(needed), "missing {needed:?} in\n{text}");
        }
    }

    #[test]
    fn issued_number_is_printed() {
        let mut doc = sample();
        doc.number = Some("RE-2026-0001".into());
        let text = joined(&layout(&doc, None));
        assert!(text.contains("RE-2026-0001"));
        assert!(!text.contains("ENTWURF"));
    }

    #[test]
    fn totals_right_aligned_at_margin() {
        let pages = layout(&sample(), None);
        let amounts: Vec<_> = texts(&pages)
            .into_iter()
            .filter(|t| ["25,00 €", "4,75 €", "29,75 €"].contains(&t.3.as_str()))
            .collect();
        assert!(amounts.len() >= 3);
        for (x, _, align, text) in amounts {
            assert_eq!((align, x), (Align::Right, mm(190.0)), "{text}");
        }
    }

    #[test]
    fn small_business_has_no_vat_column_and_note() {
        let doc = doc_with(|b, _, _| b.small_business = true);
        let pages = layout(&doc, None);
        let all = texts(&pages);
        assert!(!all.iter().any(|t| t.3 == "USt"));
        let text = joined(&pages);
        assert!(!text.contains("Summe netto"));
        assert!(!text.contains("19 %"));
        assert!(text.contains("Gemäß § 19 UStG wird keine Umsatzsteuer berechnet."));
    }

    #[test]
    fn reverse_charge_prints_both_vat_ids_and_note() {
        let doc = doc_with(|b, c, input| {
            b.vat_id = "DE123456789".into();
            c.as_mut().unwrap().vat_id = "ATU12345678".into();
            input.lines[0].tax = TaxChoice::Fixed(TaxCategory::ReverseCharge);
        });
        let text = joined(&layout(&doc, None));
        assert!(text.contains("DE123456789"));
        assert!(text.contains("ATU12345678"));
        assert!(text.contains("Steuerschuldnerschaft des Leistungsempfängers (§ 13b UStG)."));
    }

    #[test]
    fn no_customer_leaves_window_empty() {
        let doc = doc_with(|_, c, _| *c = None);
        let pages = layout(&doc, None);
        assert!(
            !texts(&pages)
                .iter()
                .any(|(x, y, _, _)| *x < mm(105.0) && *y > mm(55.0) && *y < mm(90.0))
        );
    }

    #[test]
    fn draft_has_watermark_and_draft_number() {
        let pages = layout(&sample(), None);
        assert!(
            pages[0]
                .ops
                .iter()
                .any(|op| matches!(op, Op::Watermark { text } if text == "ENTWURF"))
        );
        assert!(joined(&pages).contains("Entwurf"));
    }

    fn with_lines(count: usize, extra: Option<&str>) -> InvoiceDoc {
        let mut doc = sample();
        doc.brand.iban = "DE12 3456 7890 1234 5678 90".into();
        let template = doc.lines[0].clone();
        for i in 1..count {
            let mut line = template.clone();
            line.description = format!("Leistung {i}");
            doc.lines.push(line);
        }
        if let Some(description) = extra {
            let mut line = template.clone();
            line.description = description.into();
            doc.lines.push(line);
        }
        doc
    }

    fn page_of(pages: &[PageOps], needle: &str) -> usize {
        pages
            .iter()
            .position(|p| {
                p.ops
                    .iter()
                    .any(|op| matches!(op, Op::Text { text, .. } if text == needle))
            })
            .unwrap_or_else(|| panic!("{needle:?} not found"))
    }

    #[test]
    fn sixty_lines_need_several_pages() {
        let pages = layout(&with_lines(60, None), None);
        let n = pages.len();
        assert!(n >= 2);
        for (i, page) in pages.iter().enumerate() {
            let text = joined(std::slice::from_ref(page));
            if text.contains("Leistung") {
                assert!(text.contains("Bezeichnung"), "page {i} has no table head");
            }
            assert!(
                text.contains("IBAN DE12 3456 7890 1234 5678 90"),
                "page {i}"
            );
            assert!(
                text.contains(&format!("Seite {} von {n}", i + 1)),
                "page {i}"
            );
        }
    }

    #[test]
    fn nothing_below_body_limit() {
        let long = "Langer Text ".repeat(170);
        let longer = "Sehr langer Text ".repeat(300);
        for doc in [
            with_lines(60, None),
            with_lines(3, Some(&long)),
            with_lines(3, Some(&longer)),
        ] {
            let pages = layout(&doc, None);
            for page in &pages {
                for op in &page.ops {
                    match op {
                        Op::Text {
                            x,
                            y,
                            size,
                            bold,
                            align,
                            text,
                            ..
                        } => {
                            let footer = *y >= mm(265.0);
                            assert!(footer || *y <= mm(262.0), "{text:?} at {}", y / mm(1.0));
                            let font = if *bold {
                                &Fonts::get().bold
                            } else {
                                &Fonts::get().regular
                            };
                            let (left, right) = match align {
                                Align::Left => (*x, x + text_width(font, text, *size, false)),
                                Align::Right => (x - text_width(font, text, *size, true), *x),
                            };
                            assert!(left >= mm(20.0) - 0.01, "{text:?}");
                            assert!(right <= mm(190.0) + 0.01, "{text:?}");
                        }
                        Op::Rect { y, h, .. } => assert!(y + h <= mm(262.0) + 0.01),
                        _ => {}
                    }
                }
            }
        }
    }

    #[test]
    fn position_is_never_split() {
        let five = "Teil eins\nTeil zwei\nTeil drei\nTeil vier\nTeil fünf";
        for before in 20..70 {
            let pages = layout(&with_lines(before, Some(five)), None);
            assert_eq!(
                page_of(&pages, "Teil eins"),
                page_of(&pages, "Teil fünf"),
                "split after {before} lines"
            );
            let last_line = texts(&pages)
                .into_iter()
                .find(|t| t.3 == "Teil fünf")
                .unwrap();
            assert!(
                last_line.1 <= mm(262.0),
                "below the body after {before} lines"
            );
            if before >= 60 {
                assert!(pages.len() >= 2);
            }
        }
    }

    #[test]
    fn totals_block_stays_together() {
        for count in 20..70 {
            let pages = layout(&with_lines(count, None), None);
            let last = pages.len() - 1;
            assert_eq!(page_of(&pages, "Summe netto"), last, "{count} lines");
            assert_eq!(page_of(&pages, "Gesamtbetrag"), last, "{count} lines");
            let total = texts(&pages)
                .into_iter()
                .find(|t| t.3 == "Gesamtbetrag")
                .unwrap();
            assert!(
                total.1 <= mm(262.0),
                "total below the body with {count} lines"
            );
        }
    }

    fn with_template(template: &str, color: &str) -> Vec<PageOps> {
        let mut doc = sample();
        doc.template = template.into();
        doc.brand.color = color.into();
        layout(&doc, None)
    }

    fn rects(pages: &[PageOps]) -> Vec<(f32, f32, f32, Rgb)> {
        pages
            .iter()
            .flat_map(|p| &p.ops)
            .filter_map(|op| match op {
                Op::Rect { x, y, w, fill, .. } => Some((*x, *y, *w, *fill)),
                _ => None,
            })
            .collect()
    }

    fn color_of(pages: &[PageOps], needle: &str) -> Rgb {
        pages
            .iter()
            .flat_map(|p| &p.ops)
            .find_map(|op| match op {
                Op::Text { text, color, .. } if text == needle => Some(*color),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn modern_has_color_bar_and_colored_header() {
        let pages = with_template("modern", "#1e40af");
        let blue = Rgb(30, 64, 175);
        let all = rects(&pages);
        assert!(
            all.iter()
                .any(|r| r.1 == 0.0 && r.2 >= mm(210.0) - 0.1 && r.3 == blue)
        );
        assert!(
            all.iter().any(|r| r.1 > mm(100.0) && r.3 == blue),
            "table head"
        );
        assert_eq!(color_of(&pages, "Rechnung"), blue);
        assert_eq!(color_of(&pages, "Bezeichnung"), Rgb(255, 255, 255));
    }

    #[test]
    fn mid_light_brand_color_gets_dark_header_text() {
        let pages = with_template("modern", "#cba6f7");
        assert_eq!(color_of(&pages, "Bezeichnung"), Rgb(33, 33, 33));
    }

    #[test]
    fn light_brand_color_is_darkened_for_text() {
        let pages = with_template("modern", "#ffeb3b");
        let title = color_of(&pages, "Rechnung");
        assert_ne!(title, Rgb(255, 235, 59));
        assert!(contrast(title, Rgb(255, 255, 255)) >= 4.5);
        let total = color_of(&pages, "Gesamtbetrag");
        assert!(contrast(total, Rgb(255, 255, 255)) >= 4.5);
    }

    #[test]
    fn modern_light_color_uses_dark_header_text() {
        let pages = with_template("modern", "#ffeb3b");
        assert_ne!(color_of(&pages, "Bezeichnung"), Rgb(255, 255, 255));
    }

    #[test]
    fn minimal_has_no_filled_rects() {
        let pages = layout(&with_lines(5, None), None);
        assert!(!rects(&pages).is_empty(), "classic fills");
        let mut doc = with_lines(5, None);
        doc.template = "minimal".into();
        let pages = layout(&doc, None);
        assert!(rects(&pages).is_empty());
        let hairlines = pages[0]
            .ops
            .iter()
            .filter(|op| {
                matches!(op, Op::Line { width, color, .. }
                    if *width == 0.25 && *color == Rgb(220, 220, 220))
            })
            .count();
        assert!(hairlines >= 4, "hairlines between positions: {hairlines}");
        assert!(
            pages[0]
                .ops
                .iter()
                .any(|op| matches!(op, Op::Line { width, .. } if *width == 0.5))
        );
    }

    #[test]
    fn invalid_color_falls_back_to_grey() {
        let pages = with_template("modern", "blau");
        assert!(
            rects(&pages)
                .iter()
                .any(|r| r.1 == 0.0 && r.3 == Rgb(90, 90, 90))
        );
        let short = with_template("modern", "#f80");
        assert!(
            rects(&short)
                .iter()
                .any(|r| r.1 == 0.0 && r.3 == Rgb(255, 136, 0))
        );
    }

    #[test]
    fn unknown_template_is_classic() {
        let pages = with_template("fancy", "#3b82f6");
        let all = rects(&pages);
        assert!(!all.iter().any(|r| r.1 == 0.0));
        assert!(all.iter().any(|r| r.3 == Rgb(245, 245, 245)));
    }

    /// Left and right edge of a text op in mm.
    fn edges(op: &Op) -> Option<(f32, f32, String)> {
        let Op::Text {
            x,
            size,
            bold,
            tabular,
            align,
            text,
            ..
        } = op
        else {
            return None;
        };
        let font = if *bold {
            &Fonts::get().bold
        } else {
            &Fonts::get().regular
        };
        let w = text_width(font, text, *size, *tabular);
        let (l, r) = match align {
            Align::Left => (*x, x + w),
            Align::Right => (x - w, *x),
        };
        Some((l / mm(1.0), r / mm(1.0), text.clone()))
    }

    fn find<'a>(pages: &'a [PageOps], start: &str) -> Vec<&'a Op> {
        pages
            .iter()
            .flat_map(|p| &p.ops)
            .filter(|op| matches!(op, Op::Text { text, .. } if text.starts_with(start)))
            .collect()
    }

    #[test]
    fn long_unit_is_fitted_into_column() {
        let mut doc = sample();
        doc.lines[0].unit = "Veranstaltung".into();
        let pages = layout(&doc, None);
        let op = find(&pages, "Veranst")[0];
        let (left, right, _) = edges(op).unwrap();
        assert!(left >= 123.0 && right <= 136.0 + 0.01, "{left}..{right}");
    }

    #[test]
    fn long_brand_name_stays_clear_of_logo() {
        let mut doc = sample();
        doc.brand.name = "Die ausgesprochen lange Bezeichnung der Beispielband GbR".into();
        let pages = layout(&doc, Some((240, 120)));
        let logo_x = pages[0]
            .ops
            .iter()
            .find_map(|op| match op {
                Op::Logo { x, .. } => Some(x / mm(1.0)),
                _ => None,
            })
            .unwrap();
        let (_, right, _) = edges(find(&pages, "Die ausgesprochen")[0]).unwrap();
        assert!(right <= logo_x - 5.0 + 0.01, "{right} vs logo at {logo_x}");
    }

    #[test]
    fn huge_amounts_fit_their_columns() {
        let mut doc = sample();
        doc.lines[0].unit_price = rust_decimal::dec!(12345.6789);
        doc.totals.line_amounts = vec![Cents(1_543_827_146)];
        doc.totals.net = Cents(1_543_827_146);
        doc.totals.groups[0].base = Cents(1_543_827_146);
        doc.totals.groups[0].tax = Cents(293_327_158);
        doc.totals.tax = Cents(293_327_158);
        doc.totals.gross = Cents(1_837_154_304);
        let pages = layout(&doc, None);
        let (left, _, _) = edges(find(&pages, "12.345,6789")[0]).unwrap();
        assert!(left >= 148.0, "unit price starts at {left}");
        for op in find(&pages, "15.438.271,46") {
            let (left, _, _) = edges(op).unwrap();
            assert!(left >= 120.0, "amount starts at {left}");
        }
        // Label and value in the totals block do not overlap.
        let label = edges(find(&pages, "zzgl. USt")[0]).unwrap();
        let value = edges(find(&pages, "2.933.271,58")[0]).unwrap();
        assert!(label.1 < value.0, "{label:?} overlaps {value:?}");
    }

    #[test]
    fn notes_are_not_printed() {
        let doc = doc_with(|_, _, input| input.notes = "intern-xyz".into());
        assert!(!joined(&layout(&doc, None)).contains("intern-xyz"));
    }
}
