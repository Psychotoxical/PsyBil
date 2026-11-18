import { create } from "zustand";
import type { InvoiceFilter } from "../ipc/bindings/InvoiceFilter";
import type { InvoiceRow } from "../ipc/bindings/InvoiceRow";
import type { StatusKind } from "../ipc/bindings/StatusKind";

export type SortKey = "number" | "date" | "customer" | "amount" | "status";
export type Sort = { key: SortKey; dir: "asc" | "desc" };

export const EMPTY_FILTER: InvoiceFilter = {
  business_id: null,
  status: null,
  year: null,
  text: "",
};

const STATUS_ORDER: StatusKind[] = [
  "draft",
  "open",
  "partly_paid",
  "paid",
  "overpaid",
  "cancelled",
];

type State = {
  /** Sent to the core as it is; the list does no filtering of its own. */
  filter: InvoiceFilter;
  sort: Sort;
  setFilter: (patch: Partial<InvoiceFilter>) => void;
  /** The same column again turns the order around; a new column starts ascending. */
  sortBy: (key: SortKey) => void;
  reset: () => void;
};

/** Filter and order of the invoice list; they stay while other pages are open. */
export const useInvoices = create<State>((set, get) => ({
  filter: EMPTY_FILTER,
  sort: { key: "date", dir: "desc" },
  setFilter: (patch) => set({ filter: { ...get().filter, ...patch } }),
  sortBy: (key) => {
    const { sort } = get();
    set({
      sort:
        sort.key === key ? { key, dir: sort.dir === "asc" ? "desc" : "asc" } : { key, dir: "asc" },
    });
  },
  reset: () => set({ filter: EMPTY_FILTER, sort: { key: "date", dir: "desc" } }),
}));

const byNumber = (a: InvoiceRow, b: InvoiceRow) =>
  (a.number ?? "").localeCompare(b.number ?? "", undefined, { numeric: true });

/** Rows in the chosen order; ties fall back to date and number. */
export function sortRows(rows: InvoiceRow[], sort: Sort): InvoiceRow[] {
  const compare = (a: InvoiceRow, b: InvoiceRow): number => {
    switch (sort.key) {
      case "number":
        return byNumber(a, b);
      case "date":
        return a.invoice_date.localeCompare(b.invoice_date) || byNumber(a, b);
      case "customer":
        return (a.customer_name ?? "").localeCompare(b.customer_name ?? "");
      case "amount":
        return a.gross - b.gross;
      case "status":
        return STATUS_ORDER.indexOf(a.status.kind) - STATUS_ORDER.indexOf(b.status.kind);
    }
  };
  const sign = sort.dir === "asc" ? 1 : -1;
  return [...rows].sort(
    (a, b) => sign * compare(a, b) || b.invoice_date.localeCompare(a.invoice_date),
  );
}
