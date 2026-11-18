import type { ImportPlan } from "./bindings/ImportPlan";
import type { ImportReport } from "./bindings/ImportReport";
import type { LegacyInvoiceRow } from "./bindings/LegacyInvoiceRow";
import { call } from "./errors";

/** An old customer or item that looks like one PsyBil has already. */
export type LegacyMatch = {
  legacy_id: number;
  legacy_name: string;
  existing_id: string;
  existing_name: string;
};

/** What `import_cmd::read_legacy` sends. */
export type LegacyPreview = {
  token: string;
  path: string;
  sellers: { id: number; name: string }[];
  invoices: LegacyInvoiceRow[];
  customer_matches: LegacyMatch[];
  product_matches: LegacyMatch[];
  warnings: string[];
  plan: ImportPlan;
};

/** Old VibeBill databases in their usual place on this computer. */
export const findLegacyDatabases = () => call<string[]>("find_legacy_databases");
/** Reads a copy of the old database; the file itself stays untouched. */
export const readLegacy = (path: string) => call<LegacyPreview>("read_legacy", { path });
export const applyImport = (token: string, plan: ImportPlan) =>
  call<ImportReport>("apply_import", { token, plan });
export const cancelImport = () => call<void>("cancel_import");
export const attachOriginalPdf = (invoiceId: string, path: string) =>
  call<void>("attach_original_pdf", { invoice_id: invoiceId, path });
export const originalPdfName = (invoiceId: string) =>
  call<string | null>("original_pdf_name", { invoice_id: invoiceId });
export const exportOriginalPdf = (invoiceId: string, path: string) =>
  call<string>("export_original_pdf", { invoice_id: invoiceId, path });
