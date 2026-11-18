import type { Payment } from "./bindings/Payment";
import type { PaymentMethod } from "./bindings/PaymentMethod";
import { call } from "./errors";

export const listPayments = (invoiceId: string) =>
  call<Payment[]>("list_payments", { invoice_id: invoiceId });
/** `amount` is a decimal string with a dot ("12.50"); the core refuses fractions of cents. */
export const addPayment = (
  invoiceId: string,
  amount: string,
  date: string,
  method: PaymentMethod,
  note: string,
) => call<string>("add_payment", { invoice_id: invoiceId, amount, date, method, note });
export const deletePayment = (id: string) => call<void>("delete_payment", { id });
