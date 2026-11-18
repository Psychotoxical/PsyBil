//! Master data: businesses (the tax subject), their brands, customers and
//! the product catalog.

use chrono::NaiveDate;
use rusqlite::{Connection, Row, Transaction, params};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::audit;
use crate::calc::PriceMode;
use crate::dates::{Clock, timestamp};
use crate::db::{
    UnknownValue, dec_col, json_col, not_found, opt_dec_col, price_mode_col, uuid_col,
};
use crate::tax::TaxChoice;
use crate::tombstone::{delete_drafts_where, mark_deleted};
use crate::{Error, Result};

pub const DEFAULT_NUMBER_TEMPLATE: &str = "{PREFIX}-{YYYY}-{NNNN}";

// ---------------------------------------------------------------- business

/// Editable fields of a business.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewBusiness {
    pub name: String,
    pub street: String,
    pub zip: String,
    pub city: String,
    pub country: String,
    pub tax_number: String,
    pub vat_id: String,
    pub small_business: bool,
    pub small_business_since: Option<NaiveDate>,
    pub currency: String,
    pub number_template: String,
    pub storno_prefix: String,
    pub tax_rates: Vec<Decimal>,
}

impl Default for NewBusiness {
    fn default() -> Self {
        NewBusiness {
            name: String::new(),
            street: String::new(),
            zip: String::new(),
            city: String::new(),
            country: "DE".into(),
            tax_number: String::new(),
            vat_id: String::new(),
            small_business: false,
            small_business_since: None,
            currency: "EUR".into(),
            number_template: DEFAULT_NUMBER_TEMPLATE.into(),
            storno_prefix: "ST".into(),
            tax_rates: vec![Decimal::from(19), Decimal::from(7)],
        }
    }
}

/// A business: the person or company that owes the taxes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Business {
    pub id: Uuid,
    pub name: String,
    pub street: String,
    pub zip: String,
    pub city: String,
    pub country: String,
    pub tax_number: String,
    pub vat_id: String,
    pub small_business: bool,
    pub small_business_since: Option<NaiveDate>,
    pub currency: String,
    pub number_template: String,
    pub storno_prefix: String,
    pub tax_rates: Vec<Decimal>,
    pub archived: bool,
}

const BUSINESS_COLS: &str = "id, name, street, zip, city, country, tax_number, vat_id, \
    small_business, small_business_since, currency, number_template, storno_prefix, tax_rates, \
    archived";

fn business_from_row(r: &Row) -> rusqlite::Result<Business> {
    Ok(Business {
        id: uuid_col(r, 0)?,
        name: r.get(1)?,
        street: r.get(2)?,
        zip: r.get(3)?,
        city: r.get(4)?,
        country: r.get(5)?,
        tax_number: r.get(6)?,
        vat_id: r.get(7)?,
        small_business: r.get(8)?,
        small_business_since: r.get(9)?,
        currency: r.get(10)?,
        number_template: r.get(11)?,
        storno_prefix: r.get(12)?,
        tax_rates: json_col(r, 13)?,
        archived: r.get(14)?,
    })
}

/// Creates a business together with a brand of the same name.
/// Returns `(business id, brand id)`.
pub fn create_business(
    tx: &Transaction,
    clock: &dyn Clock,
    input: NewBusiness,
) -> Result<(Uuid, Uuid)> {
    check_template(&input.number_template)?;
    let id = Uuid::new_v4();
    let now = timestamp(clock);
    tx.execute(
        &format!(
            "INSERT INTO businesses ({BUSINESS_COLS}, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 0, ?15, ?15)"
        ),
        params![
            id.to_string(),
            input.name,
            input.street,
            input.zip,
            input.city,
            input.country,
            input.tax_number,
            input.vat_id,
            input.small_business,
            input.small_business_since,
            input.currency,
            input.number_template,
            input.storno_prefix,
            serde_json::to_string(&input.tax_rates)?,
            now,
        ],
    )?;
    audit::log(
        tx,
        clock,
        "business",
        id,
        "create",
        serde_json::to_value(&input)?,
    )?;
    let brand_id = create_brand(
        tx,
        clock,
        NewBrand {
            business_id: id,
            name: input.name.clone(),
            ..NewBrand::default()
        },
    )?;
    Ok((id, brand_id))
}

pub fn get_business(conn: &Connection, id: Uuid) -> Result<Business> {
    conn.query_row(
        &format!("SELECT {BUSINESS_COLS} FROM businesses WHERE id = ?1"),
        [id.to_string()],
        business_from_row,
    )
    .map_err(not_found)
}

pub fn list_businesses(conn: &Connection, include_archived: bool) -> Result<Vec<Business>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {BUSINESS_COLS} FROM businesses WHERE archived = 0 OR ?1
         ORDER BY name COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map([include_archived], business_from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn update_business(tx: &Transaction, clock: &dyn Clock, business: &Business) -> Result<()> {
    check_template(&business.number_template)?;
    let old = get_business(tx, business.id)?;
    tx.execute(
        "UPDATE businesses SET name = ?2, street = ?3, zip = ?4, city = ?5, country = ?6,
            tax_number = ?7, vat_id = ?8, small_business = ?9, small_business_since = ?10,
            currency = ?11, number_template = ?12, storno_prefix = ?13, tax_rates = ?14,
            archived = ?15, updated_at = ?16
         WHERE id = ?1",
        params![
            business.id.to_string(),
            business.name,
            business.street,
            business.zip,
            business.city,
            business.country,
            business.tax_number,
            business.vat_id,
            business.small_business,
            business.small_business_since,
            business.currency,
            business.number_template,
            business.storno_prefix,
            serde_json::to_string(&business.tax_rates)?,
            business.archived,
            timestamp(clock),
        ],
    )?;
    audit::log(
        tx,
        clock,
        "business",
        business.id,
        "update",
        serde_json::json!({ "old": old, "new": business }),
    )
}

pub fn archive_business(
    tx: &Transaction,
    clock: &dyn Clock,
    id: Uuid,
    archived: bool,
) -> Result<()> {
    set_archived(tx, clock, "businesses", "business", id, archived)
}

/// Deletes a business with its brands, products and drafts. Refused when
/// it has issued invoices.
pub fn delete_business(tx: &Transaction, clock: &dyn Clock, id: Uuid) -> Result<()> {
    get_business(tx, id)?;
    if exists(
        tx,
        "SELECT 1 FROM invoices WHERE business_id = ?1 AND state = 'issued'",
        id,
    )? {
        return Err(Error::InUseByIssuedInvoice);
    }
    delete_drafts_where(tx, clock, "business_id", id)?;
    for brand in list_brands(tx, id)? {
        delete_brand(tx, clock, brand.id)?;
    }
    tx.execute(
        "DELETE FROM number_counters WHERE business_id = ?1",
        [id.to_string()],
    )?;
    tx.execute("DELETE FROM businesses WHERE id = ?1", [id.to_string()])?;
    mark_deleted(tx, clock, "businesses", id)?;
    audit::log(tx, clock, "business", id, "delete", serde_json::Value::Null)
}

// ------------------------------------------------------------------- brand

/// Editable fields of a brand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewBrand {
    pub business_id: Uuid,
    pub name: String,
    #[serde(skip)]
    pub logo: Option<Vec<u8>>,
    pub logo_mime: Option<String>,
    pub color: String,
    pub phone: String,
    pub email: String,
    pub website: String,
    pub bank_name: String,
    pub iban: String,
    pub bic: String,
    pub pdf_template: String,
    pub invoice_prefix: String,
    pub payment_days: u32,
    pub intro_text: String,
    pub outro_text: String,
    pub price_mode: PriceMode,
}

impl Default for NewBrand {
    fn default() -> Self {
        NewBrand {
            business_id: Uuid::nil(),
            name: String::new(),
            logo: None,
            logo_mime: None,
            color: "#8839ef".into(),
            phone: String::new(),
            email: String::new(),
            website: String::new(),
            bank_name: String::new(),
            iban: String::new(),
            bic: String::new(),
            pdf_template: "classic".into(),
            invoice_prefix: "RE".into(),
            payment_days: 14,
            intro_text: String::new(),
            outro_text: String::new(),
            price_mode: PriceMode::Net,
        }
    }
}

/// A brand: how a business appears on its invoices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Brand {
    pub id: Uuid,
    pub business_id: Uuid,
    pub name: String,
    #[serde(skip)]
    pub logo: Option<Vec<u8>>,
    pub logo_mime: Option<String>,
    pub color: String,
    pub phone: String,
    pub email: String,
    pub website: String,
    pub bank_name: String,
    pub iban: String,
    pub bic: String,
    pub pdf_template: String,
    pub invoice_prefix: String,
    pub payment_days: u32,
    pub intro_text: String,
    pub outro_text: String,
    pub price_mode: PriceMode,
    pub archived: bool,
}

const BRAND_COLS: &str = "id, business_id, name, logo, logo_mime, color, phone, email, website, \
    bank_name, iban, bic, pdf_template, invoice_prefix, payment_days, intro_text, outro_text, \
    price_mode, archived";

fn brand_from_row(r: &Row) -> rusqlite::Result<Brand> {
    Ok(Brand {
        id: uuid_col(r, 0)?,
        business_id: uuid_col(r, 1)?,
        name: r.get(2)?,
        logo: r.get(3)?,
        logo_mime: r.get(4)?,
        color: r.get(5)?,
        phone: r.get(6)?,
        email: r.get(7)?,
        website: r.get(8)?,
        bank_name: r.get(9)?,
        iban: r.get(10)?,
        bic: r.get(11)?,
        pdf_template: r.get(12)?,
        invoice_prefix: r.get(13)?,
        payment_days: r.get(14)?,
        intro_text: r.get(15)?,
        outro_text: r.get(16)?,
        price_mode: price_mode_col(r, 17)?,
        archived: r.get(18)?,
    })
}

pub fn create_brand(tx: &Transaction, clock: &dyn Clock, input: NewBrand) -> Result<Uuid> {
    let id = Uuid::new_v4();
    tx.execute(
        &format!(
            "INSERT INTO brands ({BRAND_COLS}, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                ?17, ?18, 0, ?19, ?19)"
        ),
        params![
            id.to_string(),
            input.business_id.to_string(),
            input.name,
            input.logo,
            input.logo_mime,
            input.color,
            input.phone,
            input.email,
            input.website,
            input.bank_name,
            input.iban,
            input.bic,
            input.pdf_template,
            input.invoice_prefix,
            input.payment_days,
            input.intro_text,
            input.outro_text,
            input.price_mode.as_str(),
            timestamp(clock),
        ],
    )?;
    audit::log(
        tx,
        clock,
        "brand",
        id,
        "create",
        serde_json::to_value(&input)?,
    )?;
    Ok(id)
}

pub fn get_brand(conn: &Connection, id: Uuid) -> Result<Brand> {
    conn.query_row(
        &format!("SELECT {BRAND_COLS} FROM brands WHERE id = ?1"),
        [id.to_string()],
        brand_from_row,
    )
    .map_err(not_found)
}

pub fn list_brands(conn: &Connection, business_id: Uuid) -> Result<Vec<Brand>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {BRAND_COLS} FROM brands WHERE business_id = ?1 ORDER BY name COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map([business_id.to_string()], brand_from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn update_brand(tx: &Transaction, clock: &dyn Clock, brand: &Brand) -> Result<()> {
    let old = get_brand(tx, brand.id)?;
    if old.business_id != brand.business_id
        && exists(
            tx,
            "SELECT 1 FROM invoices WHERE brand_id = ?1 AND state = 'issued'",
            brand.id,
        )?
    {
        return Err(Error::InUseByIssuedInvoice);
    }
    tx.execute(
        "UPDATE brands SET business_id = ?2, name = ?3, logo = ?4, logo_mime = ?5, color = ?6,
            phone = ?7, email = ?8, website = ?9, bank_name = ?10, iban = ?11, bic = ?12,
            pdf_template = ?13, invoice_prefix = ?14, payment_days = ?15, intro_text = ?16,
            outro_text = ?17, price_mode = ?18, archived = ?19, updated_at = ?20
         WHERE id = ?1",
        params![
            brand.id.to_string(),
            brand.business_id.to_string(),
            brand.name,
            brand.logo,
            brand.logo_mime,
            brand.color,
            brand.phone,
            brand.email,
            brand.website,
            brand.bank_name,
            brand.iban,
            brand.bic,
            brand.pdf_template,
            brand.invoice_prefix,
            brand.payment_days,
            brand.intro_text,
            brand.outro_text,
            brand.price_mode.as_str(),
            brand.archived,
            timestamp(clock),
        ],
    )?;
    audit::log(
        tx,
        clock,
        "brand",
        brand.id,
        "update",
        serde_json::json!({ "old": old, "new": brand }),
    )
}

pub fn archive_brand(tx: &Transaction, clock: &dyn Clock, id: Uuid, archived: bool) -> Result<()> {
    set_archived(tx, clock, "brands", "brand", id, archived)
}

/// Deletes a brand with its products and drafts. Refused when it has issued
/// invoices.
pub fn delete_brand(tx: &Transaction, clock: &dyn Clock, id: Uuid) -> Result<()> {
    get_brand(tx, id)?;
    if exists(
        tx,
        "SELECT 1 FROM invoices WHERE brand_id = ?1 AND state = 'issued'",
        id,
    )? {
        return Err(Error::InUseByIssuedInvoice);
    }
    delete_drafts_where(tx, clock, "brand_id", id)?;
    for product in list_products(tx, id, true)? {
        delete_product(tx, clock, product.id)?;
    }
    tx.execute("DELETE FROM brands WHERE id = ?1", [id.to_string()])?;
    mark_deleted(tx, clock, "brands", id)?;
    audit::log(tx, clock, "brand", id, "delete", serde_json::Value::Null)
}

// ---------------------------------------------------------------- customer

/// Whether a customer is a private person or a business.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomerKind {
    Private,
    Business,
    Unknown,
}

impl CustomerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CustomerKind::Private => "private",
            CustomerKind::Business => "business",
            CustomerKind::Unknown => "unknown",
        }
    }

    pub fn parse(text: &str) -> Option<CustomerKind> {
        match text {
            "private" => Some(CustomerKind::Private),
            "business" => Some(CustomerKind::Business),
            "unknown" => Some(CustomerKind::Unknown),
            _ => None,
        }
    }
}

/// Editable fields of a customer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewCustomer {
    pub name: String,
    pub street: String,
    pub zip: String,
    pub city: String,
    pub country: String,
    pub kind: CustomerKind,
    pub vat_id: String,
    pub email: String,
    pub phone: String,
    pub notes: String,
    pub language: String,
}

impl Default for NewCustomer {
    fn default() -> Self {
        NewCustomer {
            name: String::new(),
            street: String::new(),
            zip: String::new(),
            city: String::new(),
            country: "DE".into(),
            kind: CustomerKind::Unknown,
            vat_id: String::new(),
            email: String::new(),
            phone: String::new(),
            notes: String::new(),
            language: "de".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Customer {
    pub id: Uuid,
    pub name: String,
    pub street: String,
    pub zip: String,
    pub city: String,
    pub country: String,
    pub kind: CustomerKind,
    pub vat_id: String,
    pub email: String,
    pub phone: String,
    pub notes: String,
    pub language: String,
    pub archived: bool,
}

const CUSTOMER_COLS: &str =
    "id, name, street, zip, city, country, kind, vat_id, email, phone, notes, language, archived";

fn customer_from_row(r: &Row) -> rusqlite::Result<Customer> {
    let kind: String = r.get(6)?;
    Ok(Customer {
        id: uuid_col(r, 0)?,
        name: r.get(1)?,
        street: r.get(2)?,
        zip: r.get(3)?,
        city: r.get(4)?,
        country: r.get(5)?,
        kind: CustomerKind::parse(&kind).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Text,
                Box::new(UnknownValue(kind.clone())),
            )
        })?,
        vat_id: r.get(7)?,
        email: r.get(8)?,
        phone: r.get(9)?,
        notes: r.get(10)?,
        language: r.get(11)?,
        archived: r.get(12)?,
    })
}

pub fn create_customer(tx: &Transaction, clock: &dyn Clock, input: NewCustomer) -> Result<Uuid> {
    let id = Uuid::new_v4();
    tx.execute(
        &format!(
            "INSERT INTO customers ({CUSTOMER_COLS}, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 0, ?13, ?13)"
        ),
        params![
            id.to_string(),
            input.name,
            input.street,
            input.zip,
            input.city,
            input.country,
            input.kind.as_str(),
            input.vat_id,
            input.email,
            input.phone,
            input.notes,
            input.language,
            timestamp(clock),
        ],
    )?;
    audit::log(
        tx,
        clock,
        "customer",
        id,
        "create",
        serde_json::to_value(&input)?,
    )?;
    Ok(id)
}

pub fn get_customer(conn: &Connection, id: Uuid) -> Result<Customer> {
    conn.query_row(
        &format!("SELECT {CUSTOMER_COLS} FROM customers WHERE id = ?1"),
        [id.to_string()],
        customer_from_row,
    )
    .map_err(not_found)
}

pub fn list_customers(conn: &Connection, include_archived: bool) -> Result<Vec<Customer>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CUSTOMER_COLS} FROM customers WHERE archived = 0 OR ?1
         ORDER BY name COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map([include_archived], customer_from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn update_customer(tx: &Transaction, clock: &dyn Clock, customer: &Customer) -> Result<()> {
    let old = get_customer(tx, customer.id)?;
    tx.execute(
        "UPDATE customers SET name = ?2, street = ?3, zip = ?4, city = ?5, country = ?6,
            kind = ?7, vat_id = ?8, email = ?9, phone = ?10, notes = ?11, language = ?12,
            archived = ?13, updated_at = ?14
         WHERE id = ?1",
        params![
            customer.id.to_string(),
            customer.name,
            customer.street,
            customer.zip,
            customer.city,
            customer.country,
            customer.kind.as_str(),
            customer.vat_id,
            customer.email,
            customer.phone,
            customer.notes,
            customer.language,
            customer.archived,
            timestamp(clock),
        ],
    )?;
    audit::log(
        tx,
        clock,
        "customer",
        customer.id,
        "update",
        serde_json::json!({ "old": old, "new": customer }),
    )
}

pub fn archive_customer(
    tx: &Transaction,
    clock: &dyn Clock,
    id: Uuid,
    archived: bool,
) -> Result<()> {
    set_archived(tx, clock, "customers", "customer", id, archived)
}

/// Deletes a customer and their drafts. Refused when an issued invoice
/// refers to the customer.
pub fn delete_customer(tx: &Transaction, clock: &dyn Clock, id: Uuid) -> Result<()> {
    get_customer(tx, id)?;
    if exists(
        tx,
        "SELECT 1 FROM invoices WHERE customer_id = ?1 AND state = 'issued'",
        id,
    )? {
        return Err(Error::InUseByIssuedInvoice);
    }
    delete_drafts_where(tx, clock, "customer_id", id)?;
    tx.execute("DELETE FROM customers WHERE id = ?1", [id.to_string()])?;
    mark_deleted(tx, clock, "customers", id)?;
    audit::log(tx, clock, "customer", id, "delete", serde_json::Value::Null)
}

// ----------------------------------------------------------------- product

/// Goods have stock; services do not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductKind {
    Goods,
    Service,
}

impl ProductKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProductKind::Goods => "goods",
            ProductKind::Service => "service",
        }
    }

    pub fn parse(text: &str) -> Option<ProductKind> {
        match text {
            "goods" => Some(ProductKind::Goods),
            "service" => Some(ProductKind::Service),
            _ => None,
        }
    }
}

/// Editable fields of a catalog product.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewProduct {
    pub brand_id: Uuid,
    pub name: String,
    pub description: String,
    pub kind: ProductKind,
    pub unit: String,
    pub price: Decimal,
    pub price_mode: PriceMode,
    pub tax: TaxChoice,
    pub stock: Option<Decimal>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Product {
    pub id: Uuid,
    pub brand_id: Uuid,
    pub name: String,
    pub description: String,
    pub kind: ProductKind,
    pub unit: String,
    pub price: Decimal,
    pub price_mode: PriceMode,
    pub tax: TaxChoice,
    pub stock: Option<Decimal>,
    pub active: bool,
}

const PRODUCT_COLS: &str =
    "id, brand_id, name, description, kind, unit, price, price_mode, tax, stock, active";

fn product_from_row(r: &Row) -> rusqlite::Result<Product> {
    let kind: String = r.get(4)?;
    Ok(Product {
        id: uuid_col(r, 0)?,
        brand_id: uuid_col(r, 1)?,
        name: r.get(2)?,
        description: r.get(3)?,
        kind: ProductKind::parse(&kind).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(UnknownValue(kind.clone())),
            )
        })?,
        unit: r.get(5)?,
        price: dec_col(r, 6)?,
        price_mode: price_mode_col(r, 7)?,
        tax: json_col(r, 8)?,
        stock: opt_dec_col(r, 9)?,
        active: r.get(10)?,
    })
}

pub fn create_product(tx: &Transaction, clock: &dyn Clock, input: NewProduct) -> Result<Uuid> {
    let id = Uuid::new_v4();
    let now = timestamp(clock);
    tx.execute(
        &format!(
            "INSERT INTO products ({PRODUCT_COLS}, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)"
        ),
        params![
            id.to_string(),
            input.brand_id.to_string(),
            input.name,
            input.description,
            input.kind.as_str(),
            input.unit,
            input.price.to_string(),
            input.price_mode.as_str(),
            serde_json::to_string(&input.tax)?,
            input.stock.map(|s| s.to_string()),
            input.active,
            now,
        ],
    )?;
    Ok(id)
}

/// Sum of all stock bookings of a product.
pub(crate) fn booked_stock(conn: &Connection, product_id: Uuid) -> Result<Decimal> {
    let mut stmt = conn.prepare("SELECT delta FROM stock_moves WHERE product_id = ?1")?;
    let deltas = stmt.query_map([product_id.to_string()], |r| dec_col(r, 0))?;
    let mut sum = Decimal::ZERO;
    for delta in deltas {
        sum += delta?;
    }
    Ok(sum)
}

/// Turns the stored base stock into the effective stock (base + bookings).
fn with_effective_stock(conn: &Connection, mut product: Product) -> Result<Product> {
    if let Some(base) = product.stock {
        product.stock = Some(base + booked_stock(conn, product.id)?);
    }
    Ok(product)
}

/// Schema v2 data step: stock used to include the bookings; from v2 on the
/// stored value is the base and the bookings are added on reading.
pub(crate) fn rebase_stock_v2(tx: &Transaction) -> Result<()> {
    let mut stmt = tx.prepare("SELECT id, stock FROM products WHERE stock IS NOT NULL")?;
    let rows = stmt
        .query_map([], |r| Ok((uuid_col(r, 0)?, dec_col(r, 1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, stock) in rows {
        let base = stock - booked_stock(tx, id)?;
        tx.execute(
            "UPDATE products SET stock = ?2 WHERE id = ?1",
            params![id.to_string(), base.to_string()],
        )?;
    }
    Ok(())
}

pub fn get_product(conn: &Connection, id: Uuid) -> Result<Product> {
    let product = conn
        .query_row(
            &format!("SELECT {PRODUCT_COLS} FROM products WHERE id = ?1"),
            [id.to_string()],
            product_from_row,
        )
        .map_err(not_found)?;
    with_effective_stock(conn, product)
}

pub fn list_products(
    conn: &Connection,
    brand_id: Uuid,
    include_inactive: bool,
) -> Result<Vec<Product>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PRODUCT_COLS} FROM products WHERE brand_id = ?1 AND (active = 1 OR ?2)
         ORDER BY name COLLATE NOCASE"
    ))?;
    let rows = stmt
        .query_map(
            params![brand_id.to_string(), include_inactive],
            product_from_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|p| with_effective_stock(conn, p))
        .collect()
}

/// Saves a product. `product.stock` is the wanted effective stock; the base
/// is derived from it so the bookings stay intact.
pub fn update_product(tx: &Transaction, clock: &dyn Clock, product: &Product) -> Result<()> {
    let old = get_product(tx, product.id)?;
    let booked = booked_stock(tx, product.id)?;
    let base = product.stock.map(|s| s - booked);
    let changed = tx.execute(
        "UPDATE products SET brand_id = ?2, name = ?3, description = ?4, kind = ?5, unit = ?6,
            price = ?7, price_mode = ?8, tax = ?9, stock = ?10, active = ?11, updated_at = ?12
         WHERE id = ?1",
        params![
            product.id.to_string(),
            product.brand_id.to_string(),
            product.name,
            product.description,
            product.kind.as_str(),
            product.unit,
            product.price.to_string(),
            product.price_mode.as_str(),
            serde_json::to_string(&product.tax)?,
            base.map(|s| s.to_string()),
            product.active,
            timestamp(clock),
        ],
    )?;
    if changed == 0 {
        return Err(Error::NotFound);
    }
    audit::log(
        tx,
        clock,
        "product",
        product.id,
        "update",
        serde_json::json!({ "old": old, "new": product }),
    )?;
    Ok(())
}

/// Deletes a product. Lines in drafts keep their text; refused when an
/// issued invoice contains the product.
pub fn delete_product(tx: &Transaction, clock: &dyn Clock, id: Uuid) -> Result<()> {
    get_product(tx, id)?;
    let in_issued = exists(
        tx,
        "SELECT 1 FROM invoice_lines l JOIN invoices i ON i.id = l.invoice_id
         WHERE l.product_id = ?1 AND i.state = 'issued'",
        id,
    )?;
    if in_issued || exists(tx, "SELECT 1 FROM stock_moves WHERE product_id = ?1", id)? {
        return Err(Error::InUseByIssuedInvoice);
    }
    tx.execute("DELETE FROM products WHERE id = ?1", [id.to_string()])?;
    mark_deleted(tx, clock, "products", id)?;
    audit::log(tx, clock, "product", id, "delete", serde_json::Value::Null)
}

// ----------------------------------------------------------------- helpers

/// A number template must contain a counter, or every number would repeat.
fn check_template(template: &str) -> Result<()> {
    if template.contains("{N}") || template.contains("{NNNN}") {
        Ok(())
    } else {
        Err(Error::InvalidNumberTemplate)
    }
}

fn exists(conn: &Connection, sql: &str, id: Uuid) -> Result<bool> {
    Ok(
        conn.query_row(&format!("SELECT EXISTS({sql})"), [id.to_string()], |r| {
            r.get(0)
        })?,
    )
}

fn set_archived(
    tx: &Transaction,
    clock: &dyn Clock,
    table: &str,
    entity: &str,
    id: Uuid,
    archived: bool,
) -> Result<()> {
    let changed = tx.execute(
        &format!("UPDATE {table} SET archived = ?2, updated_at = ?3 WHERE id = ?1"),
        params![id.to_string(), archived, timestamp(clock)],
    )?;
    if changed == 0 {
        return Err(Error::NotFound);
    }
    audit::log(
        tx,
        clock,
        entity,
        id,
        "archive",
        serde_json::json!({ "archived": archived }),
    )
}

#[cfg(test)]
mod stock_tests {
    use super::*;
    use crate::calc::PriceMode;
    use crate::db::Db;
    use crate::tax::{TaxCategory, TaxChoice};
    use crate::testutil::{clock, insert_raw_invoice, sample_business};
    use rust_decimal::dec;

    fn goods(brand_id: Uuid, stock: Decimal) -> NewProduct {
        NewProduct {
            brand_id,
            name: "Shirt".into(),
            description: String::new(),
            kind: ProductKind::Goods,
            unit: "Stk".into(),
            price: dec!(20),
            price_mode: PriceMode::Gross,
            tax: TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(19) }),
            stock: Some(stock),
            active: true,
        }
    }

    fn book(conn: &Connection, product_id: Uuid, invoice_id: Uuid, delta: &str) {
        conn.execute(
            "INSERT INTO stock_moves (id, product_id, invoice_id, delta, at) VALUES (?1, ?2, ?3, ?4, 'x')",
            params![Uuid::new_v4().to_string(), product_id.to_string(), invoice_id.to_string(), delta],
        )
        .unwrap();
    }

    #[test]
    fn effective_stock_sums_moves() {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let id = db
            .write(|tx| create_product(tx, &clock(), goods(brand_id, dec!(5))))
            .unwrap();
        let invoice = insert_raw_invoice(db.read(), business_id, brand_id, None, "draft");
        book(db.read(), id, invoice, "-2");
        book(db.read(), id, invoice, "1");
        assert_eq!(get_product(db.read(), id).unwrap().stock, Some(dec!(4)));
        assert_eq!(
            list_products(db.read(), brand_id, false).unwrap()[0].stock,
            Some(dec!(4))
        );
    }

    #[test]
    fn update_product_sets_effective_stock() {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let id = db
            .write(|tx| create_product(tx, &clock(), goods(brand_id, dec!(5))))
            .unwrap();
        let invoice = insert_raw_invoice(db.read(), business_id, brand_id, None, "draft");
        book(db.read(), id, invoice, "-2");
        let mut product = get_product(db.read(), id).unwrap();
        product.stock = Some(dec!(10));
        db.write(|tx| update_product(tx, &clock(), &product))
            .unwrap();
        assert_eq!(get_product(db.read(), id).unwrap().stock, Some(dec!(10)));
        let base: String = db
            .read()
            .query_row(
                "SELECT stock FROM products WHERE id = ?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(base.parse::<Decimal>().unwrap(), dec!(12));
        let audited: i64 = db
            .read()
            .query_row(
                "SELECT COUNT(*) FROM audit_log WHERE entity = 'product' AND action = 'update'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(audited, 1);
    }

    #[test]
    fn migration_v2_rebases_stock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v1.psybil");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(include_str!("schema_v1.sql")).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute_batch(
                "INSERT INTO businesses (id, name, created_at, updated_at) VALUES ('b', 'B', 'x', 'x');
                 INSERT INTO brands (id, business_id, name, created_at, updated_at) VALUES ('m', 'b', 'M', 'x', 'x');
                 INSERT INTO products (id, brand_id, name, kind, price, price_mode, tax, stock, created_at, updated_at)
                   VALUES ('00000000-0000-0000-0000-000000000001', 'm', 'P', 'goods', '1', 'net', '{}', '3', 'x', 'x');
                 INSERT INTO invoices (id, business_id, brand_id, invoice_date, service_date, payment_days, due_date,
                   price_mode, currency, language, created_at, updated_at)
                   VALUES ('i', 'b', 'm', '2026-01-01', '{}', 0, '2026-01-01', 'net', 'EUR', 'de', 'x', 'x');
                 INSERT INTO stock_moves (id, product_id, invoice_id, delta, at)
                   VALUES ('s', '00000000-0000-0000-0000-000000000001', 'i', '-2', 'x');",
            )
            .unwrap();
        }
        let db = Db::open(&path).unwrap();
        let base: String = db
            .read()
            .query_row("SELECT stock FROM products", [], |r| r.get(0))
            .unwrap();
        assert_eq!(base.parse::<Decimal>().unwrap(), dec!(5));
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use crate::db::Db;
    use crate::testutil::{clock, insert_raw_invoice, sample_business};

    #[test]
    fn number_template_needs_counter() {
        let mut db = Db::open_in_memory().unwrap();
        let input = NewBusiness {
            number_template: "{PREFIX}-{YYYY}".into(),
            ..sample_business()
        };
        let result = db.write(|tx| create_business(tx, &clock(), input));
        assert!(matches!(result, Err(Error::InvalidNumberTemplate)));
        let (id, _) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let mut business = get_business(db.read(), id).unwrap();
        business.number_template = String::new();
        let result = db.write(|tx| update_business(tx, &clock(), &business));
        assert!(matches!(result, Err(Error::InvalidNumberTemplate)));
    }

    #[test]
    fn moving_brand_with_issued_invoices_refused() {
        let mut db = Db::open_in_memory().unwrap();
        let (first, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let (second, _) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        insert_raw_invoice(db.read(), first, brand_id, None, "issued");
        let mut brand = get_brand(db.read(), brand_id).unwrap();
        brand.business_id = second;
        let result = db.write(|tx| update_brand(tx, &clock(), &brand));
        assert!(matches!(result, Err(Error::InUseByIssuedInvoice)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::tax::TaxCategory;
    use crate::testutil::{
        clock, insert_raw_invoice, insert_raw_line, sample_business, sample_customer,
    };
    use rust_decimal::dec;

    fn sample_product(brand_id: Uuid) -> NewProduct {
        NewProduct {
            brand_id,
            name: "T-Shirt".into(),
            description: String::new(),
            kind: ProductKind::Goods,
            unit: "Stk".into(),
            price: dec!(20),
            price_mode: PriceMode::Gross,
            tax: TaxChoice::Fixed(TaxCategory::Standard { rate: dec!(19) }),
            stock: Some(dec!(10)),
            active: true,
        }
    }

    #[test]
    fn create_business_adds_default_brand() {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let brands = list_brands(db.read(), business_id).unwrap();
        assert_eq!(brands.len(), 1);
        assert_eq!(brands[0].id, brand_id);
        assert_eq!(brands[0].name, "Beispiel GbR");
        assert_eq!(brands[0].invoice_prefix, "RE");
        let business = get_business(db.read(), business_id).unwrap();
        assert_eq!(business.currency, "EUR");
        assert_eq!(business.tax_rates, vec![dec!(19), dec!(7)]);
    }

    #[test]
    fn update_customer_is_audited() {
        let mut db = Db::open_in_memory().unwrap();
        let id = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        let mut customer = get_customer(db.read(), id).unwrap();
        customer.city = "Neustadt".into();
        db.write(|tx| update_customer(tx, &clock(), &customer))
            .unwrap();
        assert_eq!(get_customer(db.read(), id).unwrap().city, "Neustadt");
        let count: i64 = db
            .read()
            .query_row(
                "SELECT COUNT(*) FROM audit_log WHERE entity = 'customer' AND action = 'update'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn get_missing_is_not_found() {
        let db = Db::open_in_memory().unwrap();
        assert!(matches!(
            get_customer(db.read(), Uuid::new_v4()),
            Err(Error::NotFound)
        ));
    }

    #[test]
    fn delete_customer_with_issued_invoice_refused() {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let customer_id = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        insert_raw_invoice(
            db.read(),
            business_id,
            brand_id,
            Some(customer_id),
            "issued",
        );
        let result = db.write(|tx| delete_customer(tx, &clock(), customer_id));
        assert!(matches!(result, Err(Error::InUseByIssuedInvoice)));
        assert!(get_customer(db.read(), customer_id).is_ok());
    }

    #[test]
    fn delete_customer_removes_its_drafts() {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let customer_id = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        insert_raw_invoice(db.read(), business_id, brand_id, Some(customer_id), "draft");
        db.write(|tx| delete_customer(tx, &clock(), customer_id))
            .unwrap();
        let count: i64 = db
            .read()
            .query_row("SELECT COUNT(*) FROM invoices", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn delete_product_used_in_draft_sets_null() {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let product_id = db
            .write(|tx| create_product(tx, &clock(), sample_product(brand_id)))
            .unwrap();
        let invoice_id = insert_raw_invoice(db.read(), business_id, brand_id, None, "draft");
        let line_id = insert_raw_line(db.read(), invoice_id, Some(product_id));
        db.write(|tx| delete_product(tx, &clock(), product_id))
            .unwrap();
        let (product, description): (Option<String>, String) = db
            .read()
            .query_row(
                "SELECT product_id, description FROM invoice_lines WHERE id = ?1",
                [line_id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(product, None);
        assert_eq!(description, "Artikel");
    }

    #[test]
    fn delete_product_in_issued_invoice_refused() {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let product_id = db
            .write(|tx| create_product(tx, &clock(), sample_product(brand_id)))
            .unwrap();
        let invoice_id = insert_raw_invoice(db.read(), business_id, brand_id, None, "draft");
        insert_raw_line(db.read(), invoice_id, Some(product_id));
        db.read()
            .execute(
                "UPDATE invoices SET state = 'issued', number = 'X-1' WHERE id = ?1",
                [invoice_id.to_string()],
            )
            .unwrap();
        let result = db.write(|tx| delete_product(tx, &clock(), product_id));
        assert!(matches!(result, Err(Error::InUseByIssuedInvoice)));
        assert!(get_product(db.read(), product_id).is_ok());
    }

    #[test]
    fn delete_brand_with_issued_invoice_refused() {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        insert_raw_invoice(db.read(), business_id, brand_id, None, "issued");
        assert!(matches!(
            db.write(|tx| delete_brand(tx, &clock(), brand_id)),
            Err(Error::InUseByIssuedInvoice)
        ));
        assert!(matches!(
            db.write(|tx| delete_business(tx, &clock(), business_id)),
            Err(Error::InUseByIssuedInvoice)
        ));
        assert!(get_brand(db.read(), brand_id).is_ok());
    }

    #[test]
    fn delete_business_without_invoices() {
        let mut db = Db::open_in_memory().unwrap();
        let (business_id, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        db.write(|tx| create_product(tx, &clock(), sample_product(brand_id)))
            .unwrap();
        insert_raw_invoice(db.read(), business_id, brand_id, None, "draft");
        db.write(|tx| delete_business(tx, &clock(), business_id))
            .unwrap();
        assert!(matches!(
            get_business(db.read(), business_id),
            Err(Error::NotFound)
        ));
        assert!(matches!(
            get_brand(db.read(), brand_id),
            Err(Error::NotFound)
        ));
    }

    #[test]
    fn list_customers_hides_archived() {
        let mut db = Db::open_in_memory().unwrap();
        let a = db
            .write(|tx| create_customer(tx, &clock(), sample_customer()))
            .unwrap();
        let b = db
            .write(|tx| {
                create_customer(
                    tx,
                    &clock(),
                    NewCustomer {
                        name: "Kunde B".into(),
                        ..sample_customer()
                    },
                )
            })
            .unwrap();
        db.write(|tx| archive_customer(tx, &clock(), a, true))
            .unwrap();
        let visible = list_customers(db.read(), false).unwrap();
        assert_eq!(visible.iter().map(|c| c.id).collect::<Vec<_>>(), vec![b]);
        assert_eq!(list_customers(db.read(), true).unwrap().len(), 2);
    }

    #[test]
    fn product_roundtrip_keeps_decimals_and_tax() {
        let mut db = Db::open_in_memory().unwrap();
        let (_, brand_id) = db
            .write(|tx| create_business(tx, &clock(), sample_business()))
            .unwrap();
        let mut input = sample_product(brand_id);
        input.price = dec!(9.99);
        let id = db
            .write(|tx| create_product(tx, &clock(), input.clone()))
            .unwrap();
        let product = get_product(db.read(), id).unwrap();
        assert_eq!(product.price, dec!(9.99));
        assert_eq!(product.tax, input.tax);
        assert_eq!(product.stock, Some(dec!(10)));
        assert_eq!(list_products(db.read(), brand_id, false).unwrap().len(), 1);
    }
}
