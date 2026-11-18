import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../../i18n";
import type { InvoiceRow } from "../../ipc/bindings/InvoiceRow";
import type { StatusKind } from "../../ipc/bindings/StatusKind";
import { listInvoices } from "../../ipc/invoices";
import { useInvoices } from "../../store/invoices";
import { useUi } from "../../store/ui";
import { StatusBadge } from "../../ui/StatusBadge";
import { InvoiceList } from "./InvoiceList";

vi.mock("../../ipc/invoices", () => ({ listInvoices: vi.fn(async () => ROWS) }));
vi.mock("../../ipc/masterdata", () => ({
  listBusinesses: vi.fn(async () => [
    { id: "u1", name: "Beispiel GbR", currency: "EUR" },
    { id: "u2", name: "Muster Studio", currency: "EUR" },
  ]),
}));

function row(o: Partial<InvoiceRow> & { id: string }): InvoiceRow {
  return {
    number: null,
    kind: "invoice",
    state: "issued",
    business_id: "u1",
    brand_id: "b1",
    customer_name: "Kunde A",
    invoice_date: "2026-01-01",
    due_date: "2026-01-15",
    gross: 1000,
    status: { kind: "open", open: 1000, overdue: false },
    ...o,
  };
}

const ROWS: InvoiceRow[] = [
  row({ id: "a", number: "RE-2026-0001", invoice_date: "2026-03-01", gross: 5000 }),
  row({
    id: "b",
    number: null,
    state: "draft",
    invoice_date: "2026-05-01",
    gross: 1250,
    customer_name: "Club Beispiel",
    status: { kind: "draft", open: 1250, overdue: false },
  }),
  row({ id: "c", number: "RE-2026-0002", invoice_date: "2026-04-01", gross: 99_00 }),
];

const numbersInOrder = () =>
  screen
    .getAllByRole("row")
    .slice(1)
    .map((r) => r.getAttribute("data-id"));

describe("InvoiceList", () => {
  beforeEach(() => {
    setLanguage("de");
    vi.clearAllMocks();
    useInvoices.getState().reset();
    vi.mocked(listInvoices).mockImplementation(async () => ROWS);
    useUi.setState({ route: { page: "invoices" } });
  });

  it("filters_are_sent_to_core", async () => {
    const user = userEvent.setup();
    render(<InvoiceList />);
    await screen.findByText("RE-2026-0001");
    await user.selectOptions(screen.getByLabelText("Status"), "paid");
    await user.selectOptions(screen.getByLabelText("Absender"), "u2");
    await user.type(screen.getByRole("searchbox", { name: "Rechnungen suchen" }), "0002");
    await waitFor(() =>
      expect(listInvoices).toHaveBeenLastCalledWith({
        business_id: "u2",
        status: "paid",
        year: null,
        text: "0002",
      }),
    );
  });

  it("status_badge_has_text_and_icon_for_every_kind", () => {
    const kinds: [StatusKind, string][] = [
      ["draft", "Entwurf"],
      ["open", "Offen"],
      ["partly_paid", "Teilbezahlt"],
      ["paid", "Bezahlt"],
      ["overpaid", "Überbezahlt"],
      ["cancelled", "Storniert"],
    ];
    for (const [kind, text] of kinds) {
      const { container, unmount } = render(
        <StatusBadge status={{ kind, open: 0, overdue: false }} />,
      );
      expect(screen.getByText(text)).toBeInTheDocument();
      expect(container.querySelector("svg")).not.toBeNull();
      unmount();
    }
    const { container } = render(
      <StatusBadge status={{ kind: "open", open: 100, overdue: true }} />,
    );
    expect(screen.getByText("Überfällig")).toBeInTheDocument();
    expect(container.querySelectorAll("svg")).toHaveLength(2);
  });

  it("sorting_by_amount_and_date", async () => {
    const user = userEvent.setup();
    render(<InvoiceList />);
    await screen.findByText("RE-2026-0001");
    // Newest first to start with.
    expect(numbersInOrder()).toEqual(["b", "c", "a"]);
    await user.click(screen.getByRole("button", { name: "Betrag" }));
    expect(numbersInOrder()).toEqual(["b", "a", "c"]);
    expect(screen.getByRole("columnheader", { name: "Betrag" })).toHaveAttribute(
      "aria-sort",
      "ascending",
    );
    await user.click(screen.getByRole("button", { name: "Betrag" }));
    expect(numbersInOrder()).toEqual(["c", "a", "b"]);
    await user.click(screen.getByRole("button", { name: "Datum" }));
    expect(numbersInOrder()).toEqual(["a", "c", "b"]);
  });

  it("enter_opens_row", async () => {
    const user = userEvent.setup();
    render(<InvoiceList />);
    const table = await screen.findByRole("table");
    const draft = within(table).getByText("Club Beispiel").closest("tr") as HTMLElement;
    draft.focus();
    await user.keyboard("{Enter}");
    expect(useUi.getState().route).toEqual({ page: "assistant", id: "b" });
    const issued = within(table).getByText("RE-2026-0001").closest("tr") as HTMLElement;
    issued.focus();
    await user.keyboard("{Enter}");
    expect(useUi.getState().route).toEqual({ page: "issued", id: "a" });
  });

  it("cancellation_rows_say_cancellation_not_paid", async () => {
    vi.mocked(listInvoices).mockResolvedValue([
      row({
        id: "s",
        number: "ST-2026-0001",
        kind: "storno",
        gross: -1000,
        status: { kind: "paid", open: 0, overdue: false },
      }),
    ]);
    render(<InvoiceList />);
    const storno = (await screen.findByText("ST-2026-0001")).closest("tr") as HTMLElement;
    expect(within(storno).getByText("Storno")).toBeInTheDocument();
    expect(within(storno).queryByText("Bezahlt")).toBeNull();
  });

  it("rows_name_the_sender_when_there_are_several", async () => {
    vi.mocked(listInvoices).mockResolvedValue([
      row({ id: "a", number: "RE-2026-0001" }),
      row({ id: "b", number: "RE-2026-0001", business_id: "u2" }),
    ]);
    render(<InvoiceList />);
    await screen.findAllByText("RE-2026-0001");
    const rows = screen.getAllByRole("row").slice(1);
    expect(within(rows[0] as HTMLElement).getByText("Beispiel GbR")).toBeInTheDocument();
    expect(within(rows[1] as HTMLElement).getByText("Muster Studio")).toBeInTheDocument();
  });

  it("empty_list_offers_a_new_invoice", async () => {
    vi.mocked(listInvoices).mockResolvedValueOnce([]).mockResolvedValueOnce([]);
    const user = userEvent.setup();
    render(<InvoiceList />);
    await screen.findByText("Noch keine Rechnungen");
    await user.click(screen.getByRole("button", { name: "Neue Rechnung" }));
    expect(useUi.getState().route).toEqual({ page: "assistant" });
  });
});
