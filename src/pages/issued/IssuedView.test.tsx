import { open, save } from "@tauri-apps/plugin-dialog";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../../i18n";
import type { Invoice } from "../../ipc/bindings/Invoice";
import type { Payment } from "../../ipc/bindings/Payment";
import { cancelInvoice, correctInvoice, invoiceStatus, loadInvoice } from "../../ipc/invoices";
import { attachOriginalPdf } from "../../ipc/legacy";
import { listPayments } from "../../ipc/payments";
import { exportPdf } from "../../ipc/pdf";
import { Toasts } from "../../shell/Toasts";
import { useUi } from "../../store/ui";
import { IssuedView } from "./IssuedView";

const ISSUED: Invoice = {
  id: "i1",
  business_id: "u1",
  brand_id: "b1",
  customer_id: "c1",
  kind: "invoice",
  state: "issued",
  number: "RE-2026-0007",
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
  lines: [],
  net: 1000,
  tax: 190,
  gross: 1190,
  cancels_id: null,
  cancelled_by_id: null,
  replaces_id: null,
  imported: false,
  needs_review: false,
  legacy_cancelled: false,
};

vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn(), open: vi.fn() }));
vi.mock("../../ipc/legacy", () => ({
  originalPdfName: vi.fn(async () => null),
  attachOriginalPdf: vi.fn(async () => {}),
  exportOriginalPdf: vi.fn(async (_id: string, path: string) => path),
}));
vi.mock("../../ipc/invoices", () => ({
  loadInvoice: vi.fn(async () => ISSUED),
  invoiceStatus: vi.fn().mockResolvedValue({ kind: "open", open: 1190, overdue: false }),
  cancelInvoice: vi.fn().mockResolvedValue("s1"),
  correctInvoice: vi.fn().mockResolvedValue(["s1", "d2"]),
  duplicateAsDraft: vi.fn().mockResolvedValue("d3"),
}));
vi.mock("../../ipc/payments", () => ({
  listPayments: vi.fn().mockResolvedValue([]),
  addPayment: vi.fn(),
  deletePayment: vi.fn(),
}));
vi.mock("../../ipc/pdf", () => ({
  previewPng: vi.fn().mockResolvedValue([]),
  pdfFileName: vi.fn().mockResolvedValue("RE-2026-0007.pdf"),
  exportPdf: vi.fn(async (_id: string, path: string) => path),
}));
vi.mock("../../ipc/masterdata", () => ({
  listCustomers: vi.fn(async () => [{ id: "c1", name: "Club Beispiel" }]),
}));

const renderView = (fresh = false) =>
  render(
    <>
      <IssuedView id="i1" fresh={fresh} />
      <Toasts />
    </>,
  );

describe("IssuedView", () => {
  beforeEach(() => {
    setLanguage("de");
    vi.clearAllMocks();
    vi.mocked(loadInvoice).mockImplementation(async () => ISSUED);
    vi.mocked(invoiceStatus).mockResolvedValue({ kind: "open", open: 1190, overdue: false });
    useUi.setState({ route: { page: "issued", id: "i1" }, toasts: [] });
  });

  it("fresh_issued_view_offers_correct", async () => {
    renderView(true);
    expect(await screen.findByRole("button", { name: "Korrigieren" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Zur Liste" })).toBeInTheDocument();
  });

  it("imported_invoice_offers_original_pdf", async () => {
    const user = userEvent.setup();
    vi.mocked(loadInvoice).mockResolvedValueOnce({ ...ISSUED, imported: true });
    vi.mocked(open).mockResolvedValueOnce("C:/Temp/RE-alt.pdf");
    renderView();
    await user.click(
      await screen.findByRole("button", { name: "Damals verschicktes PDF anhängen …" }),
    );
    await waitFor(() => expect(attachOriginalPdf).toHaveBeenCalledWith("i1", "C:/Temp/RE-alt.pdf"));
    expect(
      await screen.findByRole("button", { name: "Original-PDF speichern …" }),
    ).toBeInTheDocument();
  });

  it("not_imported_invoice_has_no_original_pdf", async () => {
    renderView();
    await screen.findByRole("button", { name: "PDF speichern" });
    expect(screen.queryByRole("button", { name: /Original-PDF|Damals verschickt/ })).toBeNull();
  });

  it("cancelled_invoice_shows_its_trail_and_takes_no_payment", async () => {
    const STORNO: Invoice = { ...ISSUED, id: "s1", kind: "storno", number: "ST-2026-0002" };
    vi.mocked(loadInvoice).mockImplementation(async (id: string) =>
      id === "s1" ? STORNO : { ...ISSUED, cancelled_by_id: "s1" },
    );
    vi.mocked(invoiceStatus).mockResolvedValue({ kind: "cancelled", open: 0, overdue: false });
    const user = userEvent.setup();
    renderView();
    const link = await screen.findByRole("button", { name: "Storniert durch ST-2026-0002" });
    expect(screen.queryByRole("button", { name: "Zahlung erfassen" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Stornieren" })).toBeNull();
    const heading = screen.getByRole("heading", { level: 1 });
    expect(heading.querySelector(".num")).toHaveTextContent("RE-2026-0007");
    await user.click(link);
    expect(useUi.getState().route).toEqual({ page: "issued", id: "s1" });
  });

  it("fresh_view_names_the_number", async () => {
    renderView(true);
    expect(
      await screen.findByRole("heading", { name: "Ausgestellt: RE-2026-0007 ist fertig." }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Zur Liste" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Neue Rechnung" })).toBeInTheDocument();
  });

  it("export_cancel_does_nothing", async () => {
    vi.mocked(save).mockResolvedValue(null);
    const user = userEvent.setup();
    renderView();
    await user.click(await screen.findByRole("button", { name: "PDF speichern" }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(vi.mocked(save).mock.calls[0]?.[0]).toMatchObject({ defaultPath: "RE-2026-0007.pdf" });
    expect(exportPdf).not.toHaveBeenCalled();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("export_success_toasts_path", async () => {
    vi.mocked(save).mockResolvedValue("C:/Temp/RE-2026-0007.pdf");
    const user = userEvent.setup();
    renderView();
    await user.click(await screen.findByRole("button", { name: "PDF speichern" }));
    await waitFor(() => expect(exportPdf).toHaveBeenCalledWith("i1", "C:/Temp/RE-2026-0007.pdf"));
    expect(await screen.findByRole("status")).toHaveTextContent("C:/Temp/RE-2026-0007.pdf");
  });

  it("cancel_needs_confirmation_then_refreshes_status", async () => {
    const user = userEvent.setup();
    renderView();
    await user.click(await screen.findByRole("button", { name: "Stornieren" }));
    expect(cancelInvoice).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText(/Storno/)).toBeInTheDocument();
    const before = vi.mocked(invoiceStatus).mock.calls.length;
    await user.click(within(dialog).getByRole("button", { name: "Stornieren" }));
    expect(cancelInvoice).toHaveBeenCalledWith("i1");
    await waitFor(() => expect(vi.mocked(invoiceStatus).mock.calls.length).toBeGreaterThan(before));
  });

  it("correct_opens_replacement_draft", async () => {
    const user = userEvent.setup();
    renderView();
    await user.click(await screen.findByRole("button", { name: "Korrigieren" }));
    const dialog = screen.getByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Korrigieren" }));
    expect(correctInvoice).toHaveBeenCalledWith("i1");
    await waitFor(() => expect(useUi.getState().route).toEqual({ page: "assistant", id: "d2" }));
  });

  it("transferred_payment_cannot_be_deleted", async () => {
    const payment = (o: Partial<Payment>): Payment => ({
      id: "p",
      invoice_id: "i1",
      amount: 500,
      date: "2026-11-19",
      method: "cash",
      note: "",
      transfer_of_id: null,
      transferred_to_id: null,
      ...o,
    });
    vi.mocked(listPayments).mockResolvedValue([
      payment({ id: "p1" }),
      payment({ id: "p2", amount: 300, transferred_to_id: "p9" }),
    ]);
    renderView();
    const table = await screen.findByRole("table", { name: "Zahlungen" });
    const rows = within(table).getAllByRole("row").slice(1);
    expect(rows).toHaveLength(2);
    expect(
      within(rows[0] as HTMLElement).getByRole("button", { name: /löschen/ }),
    ).toBeInTheDocument();
    expect(within(rows[1] as HTMLElement).getByText("übertragen")).toBeInTheDocument();
    expect(within(rows[1] as HTMLElement).queryByRole("button", { name: /löschen/ })).toBeNull();
  });
});
