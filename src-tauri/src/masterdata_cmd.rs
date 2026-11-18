//! Businesses, brands, customers and the catalogue.

use psybil_core::dates::SystemClock;
use psybil_core::masterdata::{
    self, Brand, Business, Customer, NewBrand, NewBusiness, NewCustomer, NewProduct, Product,
};
use tauri::State;
use uuid::Uuid;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

#[tauri::command(async, rename_all = "snake_case")]
pub fn list_businesses(
    state: State<'_, AppState>,
    include_archived: bool,
) -> CmdResult<Vec<Business>> {
    state.read(|c| masterdata::list_businesses(c, include_archived))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn create_business(state: State<'_, AppState>, input: NewBusiness) -> CmdResult<(Uuid, Uuid)> {
    state.write(|tx| masterdata::create_business(tx, &SystemClock, input))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn update_business(state: State<'_, AppState>, business: Business) -> CmdResult<()> {
    state.write(|tx| masterdata::update_business(tx, &SystemClock, &business))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn archive_business(state: State<'_, AppState>, id: Uuid, archived: bool) -> CmdResult<()> {
    state.write(|tx| masterdata::archive_business(tx, &SystemClock, id, archived))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn delete_business(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    state.write(|tx| masterdata::delete_business(tx, &SystemClock, id))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn list_brands(state: State<'_, AppState>, business_id: Uuid) -> CmdResult<Vec<Brand>> {
    state.read(|c| masterdata::list_brands(c, business_id))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn create_brand(state: State<'_, AppState>, input: NewBrand) -> CmdResult<Uuid> {
    state.write(|tx| masterdata::create_brand(tx, &SystemClock, input))
}

/// Saves a brand. The logo never travels to the window, so it is kept as
/// it is; it only changes through `set_brand_logo` and `remove_brand_logo`.
pub fn update_brand_inner(state: &AppState, mut brand: Brand) -> CmdResult<()> {
    state.write(|tx| {
        let current = masterdata::get_brand(tx, brand.id)?;
        brand.logo = current.logo;
        brand.logo_mime = current.logo_mime;
        masterdata::update_brand(tx, &SystemClock, &brand)
    })
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn update_brand(state: State<'_, AppState>, brand: Brand) -> CmdResult<()> {
    update_brand_inner(&state, brand)
}

/// The MIME type of a PNG or JPEG file, from its first bytes.
fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else {
        None
    }
}

/// Reads the image at `path` and makes it the brand's logo.
pub fn set_brand_logo_inner(state: &AppState, brand_id: Uuid, path: &str) -> CmdResult<()> {
    let bytes = std::fs::read(path).map_err(|e| CmdError::new("io", e.to_string()))?;
    let mime = image_mime(&bytes)
        .ok_or_else(|| CmdError::new("unsupported_image", "only PNG and JPEG logos"))?;
    state.write(|tx| {
        let mut brand = masterdata::get_brand(tx, brand_id)?;
        brand.logo = Some(bytes);
        brand.logo_mime = Some(mime.into());
        masterdata::update_brand(tx, &SystemClock, &brand)
    })
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn set_brand_logo(state: State<'_, AppState>, brand_id: Uuid, path: String) -> CmdResult<()> {
    set_brand_logo_inner(&state, brand_id, &path)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn remove_brand_logo(state: State<'_, AppState>, brand_id: Uuid) -> CmdResult<()> {
    state.write(|tx| {
        let mut brand = masterdata::get_brand(tx, brand_id)?;
        brand.logo = None;
        brand.logo_mime = None;
        masterdata::update_brand(tx, &SystemClock, &brand)
    })
}

/// The brand's logo as a data URL, for showing it in the window.
#[tauri::command(async, rename_all = "snake_case")]
pub fn brand_logo(state: State<'_, AppState>, brand_id: Uuid) -> CmdResult<Option<String>> {
    use base64::Engine;
    let brand = state.read(|c| masterdata::get_brand(c, brand_id))?;
    Ok(match (brand.logo, brand.logo_mime) {
        (Some(bytes), Some(mime)) => Some(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )),
        _ => None,
    })
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn archive_brand(state: State<'_, AppState>, id: Uuid, archived: bool) -> CmdResult<()> {
    state.write(|tx| masterdata::archive_brand(tx, &SystemClock, id, archived))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn delete_brand(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    state.write(|tx| masterdata::delete_brand(tx, &SystemClock, id))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn list_customers(
    state: State<'_, AppState>,
    include_archived: bool,
) -> CmdResult<Vec<Customer>> {
    state.read(|c| masterdata::list_customers(c, include_archived))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn create_customer(state: State<'_, AppState>, input: NewCustomer) -> CmdResult<Uuid> {
    state.write(|tx| masterdata::create_customer(tx, &SystemClock, input))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn update_customer(state: State<'_, AppState>, customer: Customer) -> CmdResult<()> {
    state.write(|tx| masterdata::update_customer(tx, &SystemClock, &customer))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn archive_customer(state: State<'_, AppState>, id: Uuid, archived: bool) -> CmdResult<()> {
    state.write(|tx| masterdata::archive_customer(tx, &SystemClock, id, archived))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn delete_customer(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    state.write(|tx| masterdata::delete_customer(tx, &SystemClock, id))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn list_products(
    state: State<'_, AppState>,
    brand_id: Uuid,
    include_inactive: bool,
) -> CmdResult<Vec<Product>> {
    state.read(|c| masterdata::list_products(c, brand_id, include_inactive))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn create_product(state: State<'_, AppState>, input: NewProduct) -> CmdResult<Uuid> {
    state.write(|tx| masterdata::create_product(tx, &SystemClock, input))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn update_product(state: State<'_, AppState>, product: Product) -> CmdResult<()> {
    state.write(|tx| masterdata::update_product(tx, &SystemClock, &product))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn delete_product(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    state.write(|tx| masterdata::delete_product(tx, &SystemClock, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::demo_state;

    fn first_brand(state: &AppState) -> Brand {
        let business = state
            .read(|c| masterdata::list_businesses(c, false))
            .unwrap()
            .remove(0);
        state
            .read(|c| masterdata::list_brands(c, business.id))
            .unwrap()
            .remove(0)
    }

    #[test]
    fn update_brand_keeps_logo() {
        let (state, _dirs) = demo_state();
        let mut brand = first_brand(&state);
        assert!(brand.logo.is_some());
        brand.logo = None; // as it arrives from the window
        brand.name = "Neuer Name".into();
        update_brand_inner(&state, brand.clone()).unwrap();
        let saved = state.read(|c| masterdata::get_brand(c, brand.id)).unwrap();
        assert_eq!(saved.name, "Neuer Name");
        assert!(saved.logo.is_some(), "logo kept");
    }

    #[test]
    fn set_brand_logo_refuses_other_files() {
        let (state, dirs) = demo_state();
        let brand = first_brand(&state);
        let path = dirs.0.path().join("logo.txt");
        std::fs::write(&path, b"kein Bild").unwrap();
        let error = set_brand_logo_inner(&state, brand.id, path.to_str().unwrap()).unwrap_err();
        assert_eq!(error.code, "unsupported_image");
    }
}
