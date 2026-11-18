import type { OutsideRevenue } from "./bindings/OutsideRevenue";
import type { Overview } from "./bindings/Overview";
import type { YearSummary } from "./bindings/YearSummary";
import { call } from "./errors";

/** Tiles, debtors, monthly receipts and the small-business meters; `null` means all senders. */
export const overview = (businessId: string | null) =>
  call<Overview>("overview", { business_id: businessId });

/** `amount` is a decimal string with a dot ("1234.50"). */
export const setOutsideRevenue = (businessId: string, year: number, amount: string, note: string) =>
  call<void>("set_outside_revenue", { business_id: businessId, year, amount, note });

export const outsideRevenue = (businessId: string, year: number) =>
  call<OutsideRevenue | null>("outside_revenue", { business_id: businessId, year });

export const yearSummary = (businessId: string, year: number) =>
  call<YearSummary>("year_summary", { business_id: businessId, year });
