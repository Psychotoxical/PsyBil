import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18n, setLanguage } from "../../i18n";
import type { Invoice } from "../../ipc/bindings/Invoice";
import { draftTotals, loadInvoice } from "../../ipc/invoices";
import { issueTarget, issueText } from "../../ipc/issueTexts";
import { createCustomer } from "../../ipc/masterdata";
import { previewPng } from "../../ipc/pdf";
import { useDraft } from "../../store/draft";
import { DEFAULT_SETTINGS, useSettings } from "../../store/settings";
import { useUi } from "../../store/ui";
import { Assistant } from "./Assistant";

const line = (description: string, quantity: string, unit_price: string) => ({
  kind: "free" as const,
  product_id: null,
  description,
  quantity,
  unit: "Stück",
  unit_price,
  tax: { mode: "fixed" as const, category: { type: "standard" as const, rate: "19" } },
});

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
  lines: [line("Shirt", "2", "50"), line("CD", "1", "10")],
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

const customers = [
  { id: "c1", name: "Club Beispiel", city: "Musterstadt", language: "de", archived: false },
];

vi.mock("../../ipc/invoices", () => ({
  createDraft: vi.fn().mockResolvedValue("d1"),
  saveDraft: vi.fn().mockResolvedValue(undefined),
  deleteDraft: vi.fn().mockResolvedValue(undefined),
  loadInvoice: vi.fn(async () => DRAFT),
  checkIssue: vi.fn().mockResolvedValue([{ kind: { type: "service_date" }, severity: "error" }]),
  draftTotals: vi.fn().mockResolvedValue({
    line_amounts: [10000, 1000],
    groups: [],
    net: 2500,
    tax: 475,
    gross: 2975,
  }),
  productLine: vi.fn(async (id: string) => ({
    ...line(id, "1", "20"),
    kind: "item",
    product_id: id,
  })),
}));
vi.mock("../../ipc/masterdata", () => ({
  listBusinesses: vi.fn(async () => [
    {
      id: "u1",
      name: "Beispiel GbR",
      small_business: false,
      currency: "EUR",
      tax_rates: ["19", "7"],
    },
  ]),
  listBrands: vi.fn(async () => [
    {
      id: "b1",
      business_id: "u1",
      name: "Beispiel GbR",
      color: "#8839ef",
      payment_days: 14,
      price_mode: "net",
      intro_text: "",
      outro_text: "",
      archived: false,
    },
    {
      id: "b2",
      business_id: "u1",
      name: "Muster Studio",
      color: "#40a02b",
      payment_days: 30,
      price_mode: "gross",
      intro_text: "Hallo Studio",
      outro_text: "Danke Studio",
      archived: false,
    },
  ]),
  listCustomers: vi.fn(async () => customers),
  listProducts: vi.fn(async () => [
    {
      id: "p1",
      name: "Shirt",
      kind: "goods",
      unit: "Stück",
      price: "20",
      stock: "12",
      active: true,
    },
    {
      id: "p2",
      name: "CD",
      kind: "goods",
      unit: "Stück",
      price: "12.5",
      stock: "40",
      active: true,
    },
    {
      id: "p3",
      name: "Pauschale",
      kind: "service",
      unit: "pauschal",
      price: "450",
      stock: null,
      active: true,
    },
  ]),
  createCustomer: vi.fn(async () => {
    customers.push({ id: "c9", name: "Neu GmbH", city: "", language: "en", archived: false });
    return "c9";
  }),
}));
vi.mock("../../ipc/pdf", () => ({ previewPng: vi.fn().mockResolvedValue(["AAAA", "BBBB"]) }));

const tile = (name: string) => screen.getByRole("button", { name: new RegExp(name) });
const stepTile = (name: string) =>
  within(screen.getByRole("list", { name: "Schritte" })).getByRole("button", {
    name: new RegExp(name),
  });

describe("Assistant", () => {
  beforeEach(() => {
    setLanguage("de");
    vi.clearAllMocks();
    useDraft.getState().open(null, null);
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS } });
    useUi.setState({ route: { page: "assistant" }, toasts: [] });
  });
  afterEach(() => vi.useRealTimers());

  it("issue_text_for_reversed_period", () => {
    const t = (key: string, values?: Record<string, string>) => i18n.t(key, values);
    expect(issueText({ type: "service_period_order" }, t)).toBe(
      "Der Leistungszeitraum endet, bevor er beginnt.",
    );
    expect(issueTarget({ type: "service_period_order" })).toEqual({ step: 3 });
  });

  it("footer_total_comes_from_core", async () => {
    render(<Assistant id="d1" />);
    // 2 × 50 + 10 would be 110 €; the core says 29,75 €.
    const foot = await screen.findByRole("contentinfo");
    expect(await within(foot).findByText("29,75 €")).toBeInTheDocument();
  });

  it("step_tiles_jump_and_show_done", async () => {
    const user = userEvent.setup();
    render(<Assistant />);
    const steps = screen.getByRole("list", { name: "Schritte" });
    expect(within(steps).getByRole("button", { name: /Positionen/ })).toBeDisabled();
    await user.click(await screen.findByRole("button", { name: /^Beispiel GbR/ }));
    await waitFor(() => expect(useDraft.getState().id).toBe("d1"));
    const first = within(steps).getByRole("button", { name: /Absender/ });
    expect(first).toHaveClass("done");
    expect(within(steps).getByRole("button", { name: /Kunde/ })).toHaveAttribute(
      "aria-current",
      "step",
    );
    await user.click(within(steps).getByRole("button", { name: /Positionen/ }));
    expect(within(steps).getByRole("button", { name: /Positionen/ })).toHaveAttribute(
      "aria-current",
      "step",
    );
    expect(screen.getByRole("heading", { name: "Was berechnest du?" })).toBeInTheDocument();
  });

  it("new_customer_from_step_two_is_selected", async () => {
    const user = userEvent.setup();
    render(<Assistant />);
    await user.click(await screen.findByRole("button", { name: /^Beispiel GbR/ }));
    await user.click(screen.getByRole("button", { name: "Neuen Kunden anlegen" }));
    const dialog = screen.getByRole("dialog");
    await user.type(within(dialog).getByLabelText("Name"), "Neu GmbH");
    await user.click(within(dialog).getByRole("button", { name: "Speichern" }));
    expect(createCustomer).toHaveBeenCalled();
    await waitFor(() => expect(useDraft.getState().input?.customer_id).toBe("c9"));
    expect(useDraft.getState().input?.language).toBe("en");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("gig_and_shipping_lines_speak_the_invoice_language", async () => {
    vi.mocked(loadInvoice).mockResolvedValueOnce({ ...DRAFT, language: "en" });
    const user = userEvent.setup();
    render(<Assistant id="d1" />);
    await waitFor(() => expect(useDraft.getState().input?.language).toBe("en"));
    await user.click(stepTile("Positionen"));
    await user.click(screen.getByRole("button", { name: /^Gage/ }));
    expect(useDraft.getState().input?.lines.at(-1)).toMatchObject({
      kind: "free",
      description: "Fee for the show",
      unit: "flat rate",
    });
    await user.click(screen.getByRole("button", { name: /^Versand/ }));
    expect(useDraft.getState().input?.lines.at(-1)).toMatchObject({
      kind: "shipping",
      description: "Shipping",
    });
  });

  it("preview_lists_what_is_still_missing", async () => {
    render(<Assistant id="d1" />);
    const panel = await screen.findByRole("complementary", { name: "Vorschau" });
    expect(await within(panel).findByText("Das Leistungsdatum fehlt.")).toBeInTheDocument();
  });

  it("catalog_search_filters_tiles", async () => {
    const user = userEvent.setup();
    render(<Assistant />);
    await user.click(await screen.findByRole("button", { name: /^Beispiel GbR/ }));
    await user.click(stepTile("Positionen"));
    await screen.findByRole("button", { name: /CD/ });
    await user.type(screen.getByRole("searchbox", { name: "Katalog durchsuchen" }), "sh");
    expect(tile("Shirt")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^CD/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Pauschale/ })).not.toBeInTheDocument();
    await user.click(tile("Shirt"));
    await waitFor(() => expect(useDraft.getState().input?.lines).toHaveLength(1));
  });

  it("alt_arrow_moves_line", async () => {
    const user = userEvent.setup();
    render(<Assistant id="d1" />);
    await waitFor(() => expect(useDraft.getState().input?.lines).toHaveLength(2));
    await user.click(stepTile("Positionen"));
    const second = screen.getByLabelText("Bezeichnung, Zeile 2");
    second.focus();
    await user.keyboard("{Alt>}{ArrowUp}{/Alt}");
    expect(useDraft.getState().input?.lines.map((l) => l.description)).toEqual(["CD", "Shirt"]);
    expect(document.activeElement).toBe(screen.getByLabelText("Bezeichnung, Zeile 1"));
    expect(screen.getByLabelText("Bezeichnung, Zeile 1")).toHaveValue("CD");
  });

  it("preview_is_debounced_and_shows_pages", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: false });
    render(<Assistant id="d1" />);
    await act(() => vi.advanceTimersByTimeAsync(10));
    await act(() => vi.advanceTimersByTimeAsync(400));
    expect(previewPng).not.toHaveBeenCalled();
    await act(() => vi.advanceTimersByTimeAsync(200));
    expect(previewPng).toHaveBeenCalledWith("d1", 800);
    await act(() => vi.advanceTimersByTimeAsync(10));
    const pages = screen.getAllByRole("img", { name: /Seite/ });
    expect(pages).toHaveLength(2);
    expect(pages[0]).toHaveAttribute("src", "data:image/png;base64,AAAA");
  });

  it("new_invoice_with_open_draft_starts_fresh", async () => {
    const view = render(<Assistant id="d1" />);
    await waitFor(() => expect(useDraft.getState().id).toBe("d1"));
    // Ctrl+N or the sidebar: the route loses the id.
    useUi.setState({ route: { page: "assistant" } });
    view.rerender(<Assistant />);
    await waitFor(() => expect(useDraft.getState().id).toBeNull());
    await new Promise((r) => setTimeout(r, 20));
    expect(useUi.getState().route).toEqual({ page: "assistant" });
    expect(
      screen.getByRole("heading", { name: "Wer stellt die Rechnung aus?" }),
    ).toBeInTheDocument();
  });

  it("reopening_the_same_draft_keeps_its_step", async () => {
    const { lines, ...rest } = DRAFT;
    useDraft.getState().open("d1", {
      brand_id: rest.brand_id,
      customer_id: rest.customer_id,
      invoice_date: rest.invoice_date,
      service_date: rest.service_date,
      payment_days: rest.payment_days,
      price_mode: rest.price_mode,
      language: rest.language,
      intro_text: "",
      outro_text: "",
      notes: "",
      lines,
    });
    render(<Assistant id="d1" />);
    expect(
      await screen.findByRole("heading", { name: "Wann war es, und bis wann wird gezahlt?" }),
    ).toBeInTheDocument();
  });

  it("footer_names_a_calculation_problem_instead_of_an_old_total", async () => {
    vi.mocked(draftTotals).mockRejectedValueOnce({
      code: "too_many_decimals",
      message: "too many decimal places",
      issues: [],
    });
    render(<Assistant id="d1" />);
    const foot = await screen.findByRole("contentinfo");
    expect(
      await within(foot).findByText("Menge und Preis haben höchstens vier Nachkommastellen."),
    ).toBeInTheDocument();
    expect(within(foot).queryByText("29,75 €")).toBeNull();
  });

  it("own_payment_term_keeps_what_is_typed", async () => {
    const user = userEvent.setup();
    render(<Assistant id="d1" />);
    await user.click(await screen.findByRole("button", { name: "eigenes" }));
    const days = screen.getByLabelText("Tage bis zur Zahlung");
    await user.clear(days);
    await user.type(days, "75");
    expect(useDraft.getState().input?.payment_days).toBe(75);
    expect(screen.getByLabelText("Tage bis zur Zahlung")).toHaveValue("75");
  });

  it("changing_the_sender_takes_its_terms_and_texts", async () => {
    const user = userEvent.setup();
    render(<Assistant />);
    await user.click(await screen.findByRole("button", { name: /^Beispiel GbR/ }));
    await waitFor(() => expect(useDraft.getState().id).toBe("d1"));
    await user.click(stepTile("Absender"));
    await user.click(screen.getByRole("button", { name: /Muster Studio/ }));
    expect(useDraft.getState().input).toMatchObject({
      brand_id: "b2",
      payment_days: 30,
      price_mode: "gross",
      intro_text: "Hallo Studio",
      outro_text: "Danke Studio",
    });
    // With lines on the invoice the price mode stays, so their prices keep their meaning.
    useDraft.getState().addLine(line("Frei", "1", "10"));
    await user.click(stepTile("Absender"));
    await user.click(screen.getByRole("button", { name: /^Beispiel GbR/ }));
    expect(useDraft.getState().input).toMatchObject({
      brand_id: "b1",
      payment_days: 14,
      price_mode: "gross",
    });
  });
});
