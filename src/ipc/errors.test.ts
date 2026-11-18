import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { i18n, setLanguage } from "../i18n";
import { type CmdError, messageFor, toCmdError } from "./errors";
import { listInvoices } from "./invoices";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const t = (key: string, values?: Record<string, string>) => i18n.t(key, values);

function err(code: string, message = "core text"): CmdError {
  return { code, message, issues: [] };
}

describe("errors", () => {
  beforeEach(() => setLanguage("de"));

  it("messageFor_maps_known_codes_and_falls_back", () => {
    expect(messageFor(err("in_use_by_issued_invoice"), t, "customer")).toBe(
      "Der Kunde steht auf einer ausgestellten Rechnung. Archivieren geht.",
    );
    expect(messageFor(err("unsupported_image"), t)).toBe(
      "Als Logo gehen nur PNG- und JPEG-Bilder.",
    );
    expect(messageFor(err("something_new", "Etwas ging schief."), t)).toBe("Etwas ging schief.");
    expect(messageFor(err("invalid_number_template"), t)).toBe(
      "Die Nummernvorlage hat keine laufende Nummer.",
    );
    setLanguage("en");
    expect(messageFor(err("payment_transferred"), t)).toBe(
      "This payment was moved to the replacement invoice and cannot be deleted here.",
    );
  });

  it("messageFor_names_calculation_problems", () => {
    expect(messageFor(err("too_many_decimals"), t)).toBe(
      "Menge und Preis haben höchstens vier Nachkommastellen.",
    );
    expect(messageFor(err("invalid_amount"), t)).toBe(
      "Bitte einen Betrag mit höchstens zwei Nachkommastellen eingeben.",
    );
    expect(messageFor(err("amount_too_large"), t)).toBe("Ein Betrag ist zu groß.");
    expect(messageFor(err("invalid_rate"), t)).toBe(
      "Ein Steuersatz muss zwischen 0 und 100 % liegen.",
    );
    expect(messageFor(err("negative_total"), t)).toBe("Die Summe liegt unter null.");
    expect(messageFor(err("invalid_number"), t)).toBe("Eine Zahl lässt sich nicht lesen.");
  });

  it("toCmdError_accepts_objects_and_strings", () => {
    expect(toCmdError({ code: "pdf", message: "x", issues: [] }).code).toBe("pdf");
    expect(toCmdError("plain text")).toEqual({
      code: "unknown",
      message: "plain text",
      issues: [],
    });
  });

  it("wrappers_call_one_command_with_snake_case_args", async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const filter = { business_id: null, status: null, year: 2026, text: "" };
    await listInvoices(filter);
    expect(invoke).toHaveBeenCalledWith("list_invoices", { filter });
  });

  it("wrappers_turn_rejections_into_cmd_errors", async () => {
    vi.mocked(invoke).mockRejectedValue({ code: "not_found", message: "gone", issues: [] });
    await expect(
      listInvoices({ business_id: null, status: null, year: null, text: "" }),
    ).rejects.toMatchObject({ code: "not_found" });
  });
});
