import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { setLanguage } from "./i18n";
import type { DraftInput } from "./ipc/bindings/DraftInput";
import { saveDraft } from "./ipc/invoices";
import { useDraft } from "./store/draft";

type CloseHandler = () => Promise<void>;
const close: { handler?: CloseHandler } = {};

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onCloseRequested: (handler: CloseHandler) => {
      close.handler = handler;
      return Promise.resolve(() => {});
    },
  }),
}));
vi.mock("./ipc/invoices", () => ({
  listInvoices: vi.fn().mockResolvedValue([]),
  saveDraft: vi.fn().mockResolvedValue(undefined),
  draftTotals: vi
    .fn()
    .mockResolvedValue({ line_amounts: [], groups: [], net: 0, tax: 0, gross: 0 }),
}));

const INPUT: DraftInput = {
  brand_id: "b1",
  customer_id: null,
  invoice_date: "2026-11-18",
  service_date: { type: "same_as_invoice" },
  payment_days: 14,
  price_mode: "net",
  language: "de",
  intro_text: "",
  outro_text: "",
  notes: "",
  lines: [],
};

describe("App", () => {
  it("app_renders_shell", () => {
    setLanguage("de");
    render(<App />);
    expect(screen.getByRole("navigation", { name: "Hauptmenü" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Rechnungen/ })).toBeInTheDocument();
  });

  it("app_renders_shell_in_english", () => {
    setLanguage("en");
    render(<App />);
    expect(screen.getByRole("navigation", { name: "Main menu" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Invoices/ })).toBeInTheDocument();
    setLanguage("de");
  });

  it("window_close_writes_the_waiting_draft", async () => {
    render(<App />);
    await waitFor(() => expect(close.handler).toBeDefined());
    useDraft.getState().open("d1", INPUT);
    useDraft.getState().change({ ...INPUT, notes: "noch nicht gespeichert" });
    expect(saveDraft).not.toHaveBeenCalled();
    await close.handler?.();
    expect(saveDraft).toHaveBeenCalledWith("d1", { ...INPUT, notes: "noch nicht gespeichert" });
  });
});
