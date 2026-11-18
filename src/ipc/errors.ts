import { invoke } from "@tauri-apps/api/core";
import type { Issue } from "./bindings/Issue";

/** An error from a command, as `src-tauri/src/error.rs` sends it. */
export type CmdError = { code: string; message: string; issues: Issue[] };

export type Subject = "customer" | "product" | "brand" | "business";

type T = (key: string, values?: Record<string, string>) => string;

export function toCmdError(error: unknown): CmdError {
  if (error && typeof error === "object" && "code" in error && "message" in error) {
    const e = error as Partial<CmdError>;
    return { code: String(e.code), message: String(e.message), issues: e.issues ?? [] };
  }
  return { code: "unknown", message: String(error), issues: [] };
}

const IN_USE: Record<Subject, string> = {
  customer: "The customer is on an issued invoice. You can archive it.",
  product: "The item is on an issued invoice. You can switch it off instead.",
  brand: "The sender is on an issued invoice. You can archive it.",
  business: "The business has issued invoices. You can archive it.",
};

/** A clear message for an error, in the language of the window. */
export function messageFor(error: CmdError, t: T, subject?: Subject): string {
  switch (error.code) {
    case "in_use_by_issued_invoice":
      return t(subject ? IN_USE[subject] : "This is on an issued invoice and cannot be deleted.");
    case "validation":
      return error.issues.length > 0
        ? t("Some details are still missing.")
        : t("Please check the entry: {detail}", { detail: error.message });
    case "not_found":
      return t("This no longer exists. Maybe it was deleted on another computer.");
    case "not_draft":
    case "already_issued":
      return t("This invoice has been issued and can no longer be changed.");
    case "not_cancellable":
      return t("This invoice cannot be cancelled: it is a cancellation or already cancelled.");
    case "too_many_decimals":
      return t("Quantity and price have at most four decimal places.");
    case "invalid_amount":
      return t("Please enter an amount with at most two decimal places.");
    case "amount_too_large":
      return t("An amount is too large.");
    case "invalid_rate":
      return t("A tax rate must be between 0 and 100 %.");
    case "negative_total":
      return t("The total is below zero.");
    case "invalid_number":
      return t("A number cannot be read.");
    case "invalid_number_template":
      return t("The number template has no running number.");
    case "payment_transferred":
      return t("This payment was moved to the replacement invoice and cannot be deleted here.");
    case "unsupported_image":
      return t("Only PNG and JPEG images can be used as a logo.");
    case "pdf":
      return t("The PDF could not be created: {detail}", { detail: error.message });
    case "io":
      return t("The file could not be read or written: {detail}", { detail: error.message });
    case "read_only":
      return t("Read only: another computer is editing the data right now.");
    case "lock_held":
      return t("Another computer is editing the data right now.");
    case "lock_lost":
      return t(
        "Another computer took over. Your changes stay on this computer and are merged later.",
      );
    case "no_data":
      return t("The data is not open.");
    case "backup_damaged":
      return t("The backup is damaged and cannot be restored.");
    case "not_a_backup":
      return t("This file is not a PsyBil backup.");
    case "newer_schema":
      return t("The data comes from a newer PsyBil. Please update PsyBil.");
    case "shared_file_missing":
      return t(
        "The PsyBil file is missing in the shared folder. Maybe the cloud has not finished syncing.",
      );
    case "target_not_empty":
      return t("There is PsyBil data in that folder already.");
    case "demo":
      return t("This is not possible in demo mode.");
    case "busy":
      return t("The data is busy. Please try again.");
    default:
      return error.message;
  }
}

/** Calls one command; a failure arrives as `CmdError`. */
export async function call<R>(command: string, args?: Record<string, unknown>): Promise<R> {
  try {
    return await invoke<R>(command, args);
  } catch (error) {
    throw toCmdError(error);
  }
}
