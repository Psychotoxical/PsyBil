import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../../i18n";
import type { Overview as OverviewData } from "../../ipc/bindings/Overview";
import { overview, setOutsideRevenue } from "../../ipc/overview";
import { Sidebar } from "../../shell/Sidebar";
import { useUi } from "../../store/ui";
import { useWorkspace } from "../../store/workspace";
import { Overview } from "./Overview";

const DATA: OverviewData = {
  year: 2026,
  tiles: [
    {
      currency: "EUR",
      open: 21_900,
      open_count: 2,
      overdue: 11_900,
      overdue_count: 1,
      received_this_month: 5_000,
    },
  ],
  debtors: [
    {
      invoice_id: "i1",
      number: "RE-2026-0004",
      customer_name: "Club Beispiel",
      invoice_date: "2026-10-01",
      due_date: "2026-10-15",
      days_overdue: 34,
      open: 11_900,
      currency: "EUR",
    },
    {
      invoice_id: "i2",
      number: "RE-2026-0007",
      customer_name: "Kunde A",
      invoice_date: "2026-11-18",
      due_date: "2026-12-02",
      days_overdue: -14,
      open: 10_000,
      currency: "EUR",
    },
  ],
  monthly: [
    {
      currency: "EUR",
      months: [0, 0, 123_457, 0, 0, 0, 0, 0, 0, 0, 5_000, 0],
    },
  ],
  meters: [
    {
      business_id: "u1",
      business_name: "Beispiel GbR",
      year: 2026,
      previous: 2_030_000,
      previous_level: "warning",
      current: 128_457,
      current_level: "ok",
      next_year_level: "ok",
    },
  ],
};

vi.mock("../../ipc/overview", () => ({
  overview: vi.fn(async () => DATA),
  setOutsideRevenue: vi.fn(async () => {}),
  outsideRevenue: vi.fn(async () => null),
  yearSummary: vi.fn(async () => null),
}));
vi.mock("../../ipc/invoices", () => ({ listInvoices: vi.fn(async () => []) }));
vi.mock("../../ipc/masterdata", () => ({
  listBusinesses: vi.fn(async () => [
    { id: "u1", name: "Beispiel GbR", currency: "EUR", small_business: true },
    { id: "u2", name: "Muster Studio", currency: "EUR", small_business: false },
  ]),
}));

describe("Overview", () => {
  beforeEach(() => {
    setLanguage("de");
    vi.clearAllMocks();
    useUi.setState({ route: { page: "overview" }, overviewBusiness: null, toasts: [] });
  });

  it("overview_is_start_page_and_first_menu_entry", async () => {
    expect(useUi.getInitialState().route).toEqual({ page: "overview" });
    render(<Sidebar />);
    const items = screen.getAllByRole("button").filter((b) => b.dataset.page);
    expect(items[0]).toHaveAttribute("data-page", "overview");
    expect(items[0]).toHaveTextContent("Übersicht");
    expect(items[0]).toHaveAttribute("aria-current", "page");
  });

  it("business_switch_requests_overview_for_that_business", async () => {
    const user = userEvent.setup();
    render(<Overview />);
    await waitFor(() => expect(overview).toHaveBeenCalledWith(null));
    await user.click(await screen.findByRole("button", { name: "Muster Studio" }));
    await waitFor(() => expect(overview).toHaveBeenLastCalledWith("u2"));
    expect(screen.getByRole("button", { name: "Muster Studio" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("meter_shows_level_word_and_icon_not_only_color", async () => {
    render(<Overview />);
    const meter = await screen.findByRole("region", { name: /Kleinunternehmer/ });
    const near = within(meter).getByText("nahe an der Grenze");
    expect(near.closest(".badge")?.querySelector("svg")).not.toBeNull();
    expect(within(meter).getAllByText("unter der Grenze")).toHaveLength(2);
    // Amounts in the number font, the word between them not.
    const amount = within(meter).getByText("20.300,00 €");
    expect(amount).toHaveClass("num");
    expect(amount.parentElement).toHaveTextContent("20.300,00 € von 25.000,00 €");
    expect(within(meter).getByText(/Hinweis, keine Steuerberatung/)).toBeInTheDocument();
    expect(within(meter).getAllByRole("meter")).toHaveLength(3);
  });

  it("outside_revenue_dialog_sends_decimal_amount", async () => {
    const user = userEvent.setup();
    render(<Overview />);
    await user.click(await screen.findByRole("button", { name: "Umsatz außerhalb von PsyBil …" }));
    const dialog = screen.getByRole("dialog");
    await user.type(within(dialog).getByLabelText("Betrag"), "1.234,50");
    await user.type(within(dialog).getByLabelText("Notiz"), "Merchstand");
    await user.click(within(dialog).getByRole("button", { name: "Speichern" }));
    await waitFor(() =>
      expect(setOutsideRevenue).toHaveBeenCalledWith("u1", 2026, "1234.50", "Merchstand"),
    );
  });

  it("debtor_click_opens_issued_view", async () => {
    const user = userEvent.setup();
    render(<Overview />);
    await user.click(await screen.findByRole("button", { name: /Club Beispiel/ }));
    expect(useUi.getState().route).toEqual({ page: "issued", id: "i1" });
    expect(screen.getByText("seit 34 Tagen überfällig")).toBeInTheDocument();
    expect(screen.getByText("fällig in 14 Tagen")).toBeInTheDocument();
  });

  it("chart_labels_are_core_values", async () => {
    render(<Overview />);
    const table = await screen.findByRole("table", { name: "Zahlungseingänge 2026" });
    expect(within(table).getByText("1.234,57 €")).toBeInTheDocument();
    expect(screen.getByText("219,00 €")).toBeInTheDocument();
    // Overdue tile and the overdue invoice.
    expect(screen.getAllByText("119,00 €")).toHaveLength(2);
  });

  it("receipts_chart_has_accessible_table", async () => {
    render(<Overview />);
    const table = await screen.findByRole("table", { name: "Zahlungseingänge 2026" });
    expect(within(table).getAllByRole("row")).toHaveLength(13);
    expect(within(table).getByText("März")).toBeInTheDocument();
  });

  it("overview_load_failure_offers_retry", async () => {
    const user = userEvent.setup();
    vi.mocked(overview).mockRejectedValueOnce({ code: "busy", message: "busy", issues: [] });
    render(<Overview />);
    expect(await screen.findByText("Die Übersicht lässt sich nicht laden.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Nochmal versuchen" }));
    expect(await screen.findByRole("region", { name: /Kleinunternehmer/ })).toBeInTheDocument();
  });

  it("empty_overview_new_invoice_waits_while_reading", async () => {
    vi.mocked(overview).mockResolvedValueOnce({ ...DATA, debtors: [] });
    useWorkspace.setState({
      status: {
        place: { kind: "local" },
        access: "read_only",
        holder: null,
        stale: false,
        lock_free: false,
        lock_lost: false,
        sync_error: null,
        backup_error: null,
        conflict_copies: [],
        problem: null,
        demo: false,
        setup_done: true,
      },
    });
    render(<Overview />);
    await screen.findByText("Gerade schuldet dir niemand etwas.");
    expect(screen.getByRole("button", { name: "Neue Rechnung" })).toBeDisabled();
    useWorkspace.setState({ status: null });
  });

  it("empty_overview_offers_new_invoice", async () => {
    const user = userEvent.setup();
    vi.mocked(overview).mockResolvedValueOnce({ ...DATA, debtors: [] });
    render(<Overview />);
    expect(await screen.findByText("Gerade schuldet dir niemand etwas.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Neue Rechnung" }));
    expect(useUi.getState().route).toEqual({ page: "assistant" });
  });
});
