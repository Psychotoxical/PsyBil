import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../App";
import { setLanguage } from "../i18n";
import type { Business } from "../ipc/bindings/Business";
import { listBusinesses } from "../ipc/masterdata";
import { saveSettings } from "../ipc/settings";
import { useSettings } from "../store/settings";
import { useUi } from "../store/ui";

vi.mock("../ipc/settings", () => ({
  loadSettings: vi.fn().mockResolvedValue(null),
  saveSettings: vi.fn().mockResolvedValue(undefined),
  frontendReady: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../ipc/invoices", () => ({ listInvoices: vi.fn().mockResolvedValue([]) }));
vi.mock("../ipc/masterdata", () => ({
  listBusinesses: vi.fn().mockResolvedValue([]),
  listBrands: vi.fn().mockResolvedValue([]),
  listCustomers: vi.fn().mockResolvedValue([]),
  listProducts: vi.fn().mockResolvedValue([]),
}));

const SETTINGS = {
  device_id: "d",
  device_name: "PC",
  theme: "system" as const,
  language: "de",
  sidebar_collapsed: false,
  preview_open: true,
};

describe("Sidebar", () => {
  beforeEach(() => {
    setLanguage("de");
    useSettings.setState({ settings: { ...SETTINGS } });
    useUi.setState({ route: { page: "invoices" } });
    vi.useRealTimers();
  });

  it("sidebar_collapse_is_saved", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Menü einklappen" }));
    expect(useSettings.getState().settings?.sidebar_collapsed).toBe(true);
    await vi.advanceTimersByTimeAsync(1000);
    expect(saveSettings).toHaveBeenCalledWith(expect.objectContaining({ sidebar_collapsed: true }));
    expect(screen.getByRole("button", { name: "Menü ausklappen" })).toBeInTheDocument();
  });

  it("sidebar_foot_names_the_sender_in_context", async () => {
    vi.mocked(listBusinesses).mockResolvedValue([
      { id: "u1", name: "Beispiel GbR", small_business: true },
      { id: "u2", name: "Muster Studio", small_business: false },
    ] as Business[]);
    useUi.setState({ senderId: null });
    render(<App />);
    expect(await screen.findByText("2 Absender")).toBeInTheDocument();
    act(() => useUi.getState().setSender("u2"));
    expect(await screen.findByText("Muster Studio")).toBeInTheDocument();
    expect(screen.queryByText("Kleinunternehmer · § 19")).toBeNull();
    vi.mocked(listBusinesses).mockResolvedValue([]);
  });

  it("menu_asks_before_leaving_an_unsaved_form", async () => {
    const user = userEvent.setup();
    useUi.setState({ route: { page: "settings" }, unsaved: true, pendingRoute: null });
    render(<App />);
    await user.click(screen.getByRole("button", { name: "Rechnungen" }));
    const dialog = screen.getByRole("dialog", { name: "Änderungen verwerfen?" });
    expect(useUi.getState().route.page).toBe("settings");
    await user.click(within(dialog).getByRole("button", { name: "Verwerfen" }));
    expect(useUi.getState().route.page).toBe("invoices");
  });

  it("ctrl_n_opens_new_invoice", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.keyboard("{Control>}n{/Control}");
    expect(useUi.getState().route.page).toBe("assistant");
    expect(screen.getByRole("heading", { level: 1, name: "Neue Rechnung" })).toBeInTheDocument();
  });

  it("menu_shows_pages_and_marks_the_current_one", async () => {
    const user = userEvent.setup();
    render(<App />);
    const nav = screen.getByRole("navigation", { name: "Hauptmenü" });
    for (const name of ["Rechnungen", "Kunden", "Katalog", "Absender", "Einstellungen"]) {
      expect(nav.querySelector(`[data-page]`)).not.toBeNull();
      expect(screen.getByRole("button", { name })).toBeInTheDocument();
    }
    await user.click(screen.getByRole("button", { name: "Kunden" }));
    expect(screen.getByRole("button", { name: "Kunden" })).toHaveAttribute("aria-current", "page");
  });

  it("wordmark_mask_points_at_the_bundled_svg", () => {
    render(<App />);
    const mark = screen.getByRole("img", { name: "PsyBil" });
    expect(mark.style.getPropertyValue("--wordmark")).toMatch(/^url\(".*wordmark-psybil\.svg"\)$/);
  });
});
