import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../../i18n";
import type { Invoice } from "../../ipc/bindings/Invoice";
import { checkIssue, issueInvoice } from "../../ipc/invoices";
import { useDraft } from "../../store/draft";
import { DEFAULT_SETTINGS, useSettings } from "../../store/settings";
import { useUi } from "../../store/ui";
import { Assistant } from "./Assistant";

const DRAFT: Invoice = {
  id: "d1",
  business_id: "u1",
  brand_id: "b1",
  customer_id: "c1",
  kind: "invoice",
  state: "draft",
  number: null,
  invoice_date: "2026-11-18",
  service_date: { type: "same_as_invoice" },
  payment_days: 14,
  due_date: "2026-12-02",
  price_mode: "net",
  currency: "EUR",
  language: "de",
  intro_text: "",
  outro_text: "",
  notes: "",
  lines: [
    {
      kind: "free",
      product_id: null,
      description: "",
      quantity: "1",
      unit: "Stück",
      unit_price: "10",
      tax: { mode: "fixed", category: { type: "standard", rate: "19" } },
    },
  ],
  net: null,
  tax: null,
  gross: null,
  cancels_id: null,
  cancelled_by_id: null,
  replaces_id: null,
  imported: false,
  needs_review: false,
  legacy_cancelled: false,
};

vi.mock("../../ipc/invoices", () => ({
  createDraft: vi.fn(),
  saveDraft: vi.fn().mockResolvedValue(undefined),
  deleteDraft: vi.fn(),
  loadInvoice: vi.fn(async () => DRAFT),
  draftTotals: vi
    .fn()
    .mockResolvedValue({ line_amounts: [1000], groups: [], net: 1000, tax: 190, gross: 1190 }),
  productLine: vi.fn(),
  checkIssue: vi.fn(),
  peekNumber: vi.fn().mockResolvedValue("RE-2026-0007"),
  issueInvoice: vi.fn().mockResolvedValue({
    number: "RE-2026-0007",
    totals: { line_amounts: [1000], groups: [], net: 1000, tax: 190, gross: 1190 },
    negative_stock: [],
  }),
}));
vi.mock("../../ipc/masterdata", () => ({
  listBusinesses: vi.fn(async () => [
    { id: "u1", name: "Beispiel GbR", small_business: false, currency: "EUR", tax_rates: ["19"] },
  ]),
  listBrands: vi.fn(async () => [
    { id: "b1", business_id: "u1", name: "Beispiel GbR", color: "#8839ef", archived: false },
  ]),
  listCustomers: vi.fn(async () => [
    { id: "c1", name: "Club Beispiel", city: "", language: "de", archived: false },
  ]),
  listProducts: vi.fn(async () => []),
}));
vi.mock("../../ipc/pdf", () => ({ previewPng: vi.fn().mockResolvedValue([]) }));

const steps = () => screen.getByRole("list", { name: "Schritte" });

describe("StepIssue", () => {
  beforeEach(() => {
    setLanguage("de");
    vi.clearAllMocks();
    useDraft.getState().open(null, null);
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS } });
    useUi.setState({ route: { page: "assistant", id: "d1" }, toasts: [] });
  });

  it("issue_button_disabled_lists_missing_items", async () => {
    vi.mocked(checkIssue).mockResolvedValue([
      { kind: { type: "customer_address" }, severity: "error" },
      { kind: { type: "line_description", position: 1 }, severity: "error" },
    ]);
    const user = userEvent.setup();
    render(<Assistant id="d1" />);
    await waitFor(() => expect(useDraft.getState().id).toBe("d1"));
    await user.click(within(steps()).getByRole("button", { name: /Ausstellen/ }));
    const issue = await screen.findByRole("button", { name: "2 Angaben fehlen" });
    expect(issue).toBeDisabled();
    expect(screen.getByText("Die Anschrift des Kunden ist unvollständig.")).toBeInTheDocument();
    expect(screen.getByText("Position 1 hat keine Bezeichnung.")).toBeInTheDocument();
    expect(await screen.findByText(/RE-2026-0007/)).toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: "Zum Feld" })[0] as HTMLElement);
    expect(within(steps()).getByRole("button", { name: /Kunde/ })).toHaveAttribute(
      "aria-current",
      "step",
    );
  });

  it("issue_waits_for_a_failed_save", async () => {
    vi.mocked(checkIssue).mockResolvedValue([]);
    const user = userEvent.setup();
    render(<Assistant id="d1" />);
    await waitFor(() => expect(useDraft.getState().id).toBe("d1"));
    useDraft.setState({
      save: "error",
      error: { code: "busy", message: "", issues: [] },
    });
    await user.click(within(steps()).getByRole("button", { name: /Ausstellen/ }));
    const issue = await screen.findByRole("button", { name: "Erst speichern" });
    expect(issue).toBeDisabled();
    expect(issueInvoice).not.toHaveBeenCalled();
  });

  it("issue_shows_number_and_freezes", async () => {
    vi.mocked(checkIssue).mockResolvedValue([]);
    const user = userEvent.setup();
    render(<Assistant id="d1" />);
    await waitFor(() => expect(useDraft.getState().id).toBe("d1"));
    await user.click(within(steps()).getByRole("button", { name: /Ausstellen/ }));
    await user.click(screen.getByRole("checkbox", { name: /Zahlung bereits erhalten/ }));
    const issue = await screen.findByRole("button", { name: "Ausstellen" });
    await waitFor(() => expect(issue).toBeEnabled());
    await user.click(issue);
    expect(issueInvoice).toHaveBeenCalledWith("d1", true);
    await waitFor(() =>
      expect(useUi.getState().route).toEqual({ page: "issued", id: "d1", fresh: true }),
    );
    expect(useDraft.getState().id).toBeNull();
  });
});
