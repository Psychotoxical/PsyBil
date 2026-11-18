import type { Brand } from "./bindings/Brand";
import type { Business } from "./bindings/Business";
import type { Customer } from "./bindings/Customer";
import type { NewBrand } from "./bindings/NewBrand";
import type { NewBusiness } from "./bindings/NewBusiness";
import type { NewCustomer } from "./bindings/NewCustomer";
import type { NewProduct } from "./bindings/NewProduct";
import type { Product } from "./bindings/Product";
import { call } from "./errors";

export const listBusinesses = (includeArchived = false) =>
  call<Business[]>("list_businesses", { include_archived: includeArchived });
/** Returns the business and its first brand. */
export const createBusiness = (input: NewBusiness) =>
  call<[string, string]>("create_business", { input });
export const updateBusiness = (business: Business) => call<void>("update_business", { business });
export const archiveBusiness = (id: string, archived: boolean) =>
  call<void>("archive_business", { id, archived });
export const deleteBusiness = (id: string) => call<void>("delete_business", { id });

export const listBrands = (businessId: string) =>
  call<Brand[]>("list_brands", { business_id: businessId });
export const createBrand = (input: NewBrand) => call<string>("create_brand", { input });
export const updateBrand = (brand: Brand) => call<void>("update_brand", { brand });
export const setBrandLogo = (brandId: string, path: string) =>
  call<void>("set_brand_logo", { brand_id: brandId, path });
export const removeBrandLogo = (brandId: string) =>
  call<void>("remove_brand_logo", { brand_id: brandId });
/** The logo as a data URL, or null. */
export const brandLogo = (brandId: string) =>
  call<string | null>("brand_logo", { brand_id: brandId });
export const archiveBrand = (id: string, archived: boolean) =>
  call<void>("archive_brand", { id, archived });
export const deleteBrand = (id: string) => call<void>("delete_brand", { id });

export const listCustomers = (includeArchived = false) =>
  call<Customer[]>("list_customers", { include_archived: includeArchived });
export const createCustomer = (input: NewCustomer) => call<string>("create_customer", { input });
export const updateCustomer = (customer: Customer) => call<void>("update_customer", { customer });
export const archiveCustomer = (id: string, archived: boolean) =>
  call<void>("archive_customer", { id, archived });
export const deleteCustomer = (id: string) => call<void>("delete_customer", { id });

export const listProducts = (brandId: string, includeInactive = false) =>
  call<Product[]>("list_products", { brand_id: brandId, include_inactive: includeInactive });
export const createProduct = (input: NewProduct) => call<string>("create_product", { input });
export const updateProduct = (product: Product) => call<void>("update_product", { product });
export const deleteProduct = (id: string) => call<void>("delete_product", { id });
