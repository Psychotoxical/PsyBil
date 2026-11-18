import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../App";
import { setLanguage } from "../../i18n";
import { saveSettings } from "../../ipc/settings";
import { DEFAULT_SETTINGS, useSettings } from "../../store/settings";
import { useUi } from "../../store/ui";

vi.mock("../../ipc/settings", () => ({
  loadSettings: vi.fn().mockResolvedValue(null),
  saveSettings: vi.fn().mockResolvedValue(undefined),
  frontendReady: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../ipc/masterdata", () => ({
  listBusinesses: vi.fn().mockResolvedValue([]),
  listBrands: vi.fn().mockResolvedValue([]),
}));
vi.mock("../../ipc/invoices", () => ({ listInvoices: vi.fn().mockResolvedValue([]) }));

describe("Settings", () => {
  beforeEach(() => {
    setLanguage("de");
    vi.clearAllMocks();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, theme: "light" } });
    useUi.setState({ route: { page: "settings" } });
  });
  afterEach(() => {
    vi.useRealTimers();
    setLanguage("de");
  });

  it("theme_choice_applies_and_is_saved", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(<App />);
    expect(document.documentElement.dataset.theme).toBe("latte");
    await user.click(screen.getByRole("button", { name: "Dunkel" }));
    expect(screen.getByRole("button", { name: "Dunkel" })).toHaveAttribute("aria-pressed", "true");
    expect(document.documentElement.dataset.theme).toBe("mocha");
    await vi.advanceTimersByTimeAsync(1000);
    expect(saveSettings).toHaveBeenCalledWith(expect.objectContaining({ theme: "dark" }));
  });

  it("language_switch_rerenders_texts", async () => {
    const user = userEvent.setup();
    render(<App />);
    expect(screen.getByRole("heading", { level: 1, name: "Einstellungen" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "English" }));
    expect(screen.getByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Main menu" })).toBeInTheDocument();
    expect(useSettings.getState().settings?.language).toBe("en");
  });

  it("about_shows_version_and_font_licence", () => {
    render(<App />);
    expect(screen.getByText(/0\.1\.0/)).toBeInTheDocument();
    expect(screen.getByText(/SIL Open Font License/)).toBeInTheDocument();
  });
});
