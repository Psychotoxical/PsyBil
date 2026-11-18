import { save } from "@tauri-apps/plugin-dialog";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../../i18n";
import type { YearMonth } from "../../ipc/bindings/YearMonth";
import type { YearSummary as Summary } from "../../ipc/bindings/YearSummary";
import { exportFileName, exportYearReport, revealExported } from "../../ipc/exports";
import { yearSummary } from "../../ipc/overview";
import { Toasts } from "../../shell/Toasts";
import { useUi } from "../../store/ui";
import { YearSummary } from "./YearSummary";

const month = (m: number, o: Partial<YearMonth> = {}): YearMonth => ({
  month: m,
  count: 0,
  net: 0,
  tax: 0,
  gross: 0,
  received: 0,
  ...o,
});

const SUMMARY: Summary = {
  business_id: "u1",
  business_name: "Beispiel GbR",
  year: 2025,
  currency: "EUR",
  months: Array.from({ length: 12 }, (_, i) =>
    i === 2
      ? month(3, { count: 1, net: 103_745, tax: 19_712, gross: 123_457, received: 5_000 })
      : month(i + 1),
  ),
  total: month(0, { count: 1, net: 103_745, tax: 19_712, gross: 123_457, received: 5_000 }),
  open_at: "2025-12-31",
  open_at_end: 11_900,
  open_count: 1,
  created_on: "2026-11-18",
};

vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));
vi.mock("../../ipc/overview", () => ({ yearSummary: vi.fn(async () => SUMMARY) }));
vi.mock("../../ipc/exports", () => ({
  exportYearReport: vi.fn(async (_b: string, _y: number, path: string) => path),
  exportFileName: vi.fn(async () => "Jahresübersicht-2025-Beispiel_GbR.pdf"),
  revealExported: vi.fn(async () => {}),
  exportInvoicesCsv: vi.fn(),
}));
vi.mock("../../ipc/invoices", () => ({
  listInvoices: vi.fn(async () => [{ invoice_date: "2025-03-04" }, { invoice_date: "2026-01-10" }]),
}));

const BUSINESSES = [
  { id: "u1", name: "Beispiel GbR", currency: "EUR" },
  { id: "u2", name: "Muster Studio", currency: "EUR" },
] as never;

describe("YearSummary", () => {
  beforeEach(() => {
    setLanguage("de");
    vi.clearAllMocks();
    useUi.setState({ toasts: [] });
  });

  it("year_summary_table_shows_core_values", async () => {
    render(<YearSummary businesses={BUSINESSES} business={null} year={2025} />);
    const table = await screen.findByRole("table", { name: /Jahresübersicht 2025/ });
    const march = within(table).getByRole("row", { name: /März/ });
    expect(within(march).getByText("1.037,45 €")).toBeInTheDocument();
    expect(within(march).getByText("1.234,57 €")).toBeInTheDocument();
    expect(within(table).getByRole("row", { name: /Summe/ })).toBeInTheDocument();
    expect(document.querySelector(".year-open")).toHaveTextContent("Offen am 31.12.2025");
    expect(screen.getByText("119,00 €")).toBeInTheDocument();
    expect(yearSummary).toHaveBeenCalledWith("u1", 2025);
  });

  it("year_export_passes_business_year_format_and_path", async () => {
    const user = userEvent.setup();
    vi.mocked(save).mockResolvedValueOnce("C:/Temp/j.pdf");
    render(<YearSummary businesses={BUSINESSES} business="u2" year={2025} />);
    await screen.findByRole("table");
    await user.click(screen.getByRole("button", { name: "Als PDF speichern …" }));
    await waitFor(() =>
      expect(exportYearReport).toHaveBeenCalledWith("u2", 2025, "C:/Temp/j.pdf", "pdf"),
    );
    expect(exportFileName).toHaveBeenCalledWith("year_pdf", "u2", 2025);
    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({ defaultPath: "Jahresübersicht-2025-Beispiel_GbR.pdf" }),
    );
  });

  it("toast_action_reveals_exported_file", async () => {
    const user = userEvent.setup();
    vi.mocked(save).mockResolvedValueOnce("C:/Temp/j.csv");
    render(
      <>
        <YearSummary businesses={BUSINESSES} business="u1" year={2025} />
        <Toasts />
      </>,
    );
    await screen.findByRole("table");
    await user.click(screen.getByRole("button", { name: "Als CSV speichern …" }));
    const message = await screen.findByRole("status");
    expect(message).toHaveTextContent("Gespeichert: j.csv");
    await user.click(within(message).getByRole("button", { name: "Im Ordner zeigen" }));
    expect(revealExported).toHaveBeenCalledWith("C:/Temp/j.csv");
  });

  it("save_buttons_wait_for_the_summary", async () => {
    let resolve: (s: Summary) => void = () => {};
    vi.mocked(yearSummary).mockImplementationOnce(
      () =>
        new Promise<Summary>((r) => {
          resolve = r;
        }),
    );
    render(<YearSummary businesses={BUSINESSES} business="u1" year={2025} />);
    const pdf = screen.getByRole("button", { name: "Als PDF speichern …" });
    expect(pdf).toBeDisabled();
    resolve(SUMMARY);
    await waitFor(() => expect(pdf).toBeEnabled());
  });

  it("cancelled_save_dialog_writes_nothing", async () => {
    const user = userEvent.setup();
    vi.mocked(save).mockResolvedValueOnce(null);
    render(<YearSummary businesses={BUSINESSES} business="u1" year={2025} />);
    await screen.findByRole("table");
    await user.click(screen.getByRole("button", { name: "Als PDF speichern …" }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(exportYearReport).not.toHaveBeenCalled();
    expect(useUi.getState().toasts).toEqual([]);
  });
});
