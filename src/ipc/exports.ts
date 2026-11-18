import type { InvoiceFilter } from "./bindings/InvoiceFilter";
import { call } from "./errors";

export type ExportKind = "year_pdf" | "year_csv" | "invoices_csv";

/** Writes the year summary of one business; returns where it went. */
export const exportYearReport = (
  businessId: string,
  year: number,
  path: string,
  format: "pdf" | "csv",
) => call<string>("export_year_report", { business_id: businessId, year, path, format });

/** Writes the invoices `filter` selects as CSV in the window's language. */
export const exportInvoicesCsv = (filter: InvoiceFilter, path: string) =>
  call<string>("export_invoices_csv", { filter, path });

/** The suggested file name, in the window's language. */
export const exportFileName = (kind: ExportKind, businessId?: string, year?: number) =>
  call<string>("export_file_name", {
    kind,
    business_id: businessId ?? null,
    year: year ?? null,
  });

/** Shows a file PsyBil wrote in this session in the file manager. */
export const revealExported = (path: string) => call<void>("reveal_exported", { path });
