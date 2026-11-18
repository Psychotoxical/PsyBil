import type { DraftInput } from "./bindings/DraftInput";
import type { Invoice } from "./bindings/Invoice";
import type { InvoiceFilter } from "./bindings/InvoiceFilter";
import type { InvoiceRow } from "./bindings/InvoiceRow";
import type { InvoiceStatus } from "./bindings/InvoiceStatus";
import type { Issue } from "./bindings/Issue";
import type { Issued } from "./bindings/Issued";
import type { LineDraft } from "./bindings/LineDraft";
import type { PriceMode } from "./bindings/PriceMode";
import type { Totals } from "./bindings/Totals";
import { call } from "./errors";

export const listInvoices = (filter: InvoiceFilter) =>
  call<InvoiceRow[]>("list_invoices", { filter });
export const loadInvoice = (id: string) => call<Invoice>("load_invoice", { id });
export const createDraft = (input: DraftInput) => call<string>("create_draft", { input });
export const saveDraft = (id: string, input: DraftInput) => call<void>("save_draft", { id, input });
export const deleteDraft = (id: string) => call<void>("delete_draft", { id });
export const draftTotals = (id: string) => call<Totals>("draft_totals", { id });
/** A catalogue product as a new line, its price already in the draft's price mode. */
export const productLine = (productId: string, priceMode: PriceMode) =>
  call<LineDraft>("product_line", { product_id: productId, price_mode: priceMode });
export const checkIssue = (id: string) => call<Issue[]>("check_issue", { id });
export const peekNumber = (id: string) => call<string>("peek_number", { id });
export const issueInvoice = (id: string, paymentReceived: boolean) =>
  call<Issued>("issue_invoice", { id, payment_received: paymentReceived });
export const cancelInvoice = (id: string) => call<string>("cancel_invoice", { id });
/** Returns the cancellation and the replacement draft. */
export const correctInvoice = (id: string) => call<[string, string]>("correct_invoice", { id });
export const duplicateAsDraft = (id: string) => call<string>("duplicate_as_draft", { id });
export const invoiceStatus = (id: string) => call<InvoiceStatus>("invoice_status", { id });
