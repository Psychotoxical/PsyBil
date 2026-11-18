import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../../i18n";
import {
  applyImport,
  cancelImport,
  findLegacyDatabases,
  type LegacyPreview,
  readLegacy,
} from "../../ipc/legacy";
import type { WorkspaceStatus } from "../../ipc/workspace";
import { WelcomeDialog } from "../../shell/WelcomeDialog";
import { useUi } from "../../store/ui";
import { useWorkspace } from "../../store/workspace";
import { ImportAssistant } from "./ImportAssistant";

const PREVIEW: LegacyPreview = {
  token: "tok",
  path: "C:/Temp/vibebill.db",
  sellers: [
    { id: 1, name: "Die Band" },
    { id: 2, name: "Studio Beispiel" },
  ],
  invoices: [
    {
      legacy_id: 1,
      seller_id: 1,
      number: "RE-2026-0001",
      date: "2026-03-01",
      customer: "Club Beispiel",
      gross: 11_900,
      old_status: "draft",
    },
    {
      legacy_id: 2,
      seller_id: 1,
      number: "RE-2026-0002",
      date: "2026-03-05",
      customer: "Kunde A",
      gross: 4_491,
      old_status: "sent",
    },
  ],
  customer_matches: [
    {
      legacy_id: 1,
      legacy_name: "Club Beispiel",
      existing_id: "c9",
      existing_name: "Club Beispiel",
    },
  ],
  product_matches: [],
  warnings: ["An item without a product was kept as a free line."],
  plan: {
    businesses: [
      { sellers: [1], name: "Die Band", small_business: false, existing: null },
      { sellers: [2], name: "Studio Beispiel", small_business: false, existing: null },
    ],
    invoices: [
      { legacy_id: 1, number: "RE-2026-0001", class: "NeverSent", review: [] },
      { legacy_id: 2, number: "RE-2026-0002", class: "SentOpen", review: ["TotalsMismatch"] },
    ],
    customer_matches: [[1, "c9"]],
    product_matches: [],
  },
};

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("../../ipc/legacy", () => ({
  findLegacyDatabases: vi.fn(async () => ["C:/Temp/vibebill.db"]),
  readLegacy: vi.fn(async () => PREVIEW),
  applyImport: vi.fn(async () => ({
    businesses: 2,
    customers: 1,
    products: 0,
    drafts: 1,
    issued: 1,
    payments: 0,
    quarantined: 0,
    needs_review: 1,
    already_present: 0,
    set_aside: [],
  })),
  cancelImport: vi.fn(async () => {}),
}));
vi.mock("../../ipc/workspace", () => ({
  finishSetup: vi.fn(async () => {}),
  chooseDataPlace: vi.fn(async () => null),
  // After the welcome dialog the app reports the finished setup.
  workspaceStatus: vi.fn(async () => ({
    place: { kind: "local" },
    access: "read_write",
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
  })),
}));

async function toSenders(user: ReturnType<typeof userEvent.setup>) {
  render(<ImportAssistant />);
  await user.click(await screen.findByRole("button", { name: /C:\/Temp\/vibebill\.db/ }));
  await waitFor(() => expect(readLegacy).toHaveBeenCalledWith("C:/Temp/vibebill.db"));
}

async function answerSmallBusiness(user: ReturnType<typeof userEvent.setup>) {
  for (const name of ["Die Band", "Studio Beispiel"]) {
    const group = screen.getByRole("group", { name: `Kleinunternehmer? ${name}` });
    await user.click(within(group).getByRole("button", { name: "Ja" }));
  }
}

describe("ImportAssistant", () => {
  beforeEach(() => {
    setLanguage("de");
    vi.clearAllMocks();
    useUi.setState({ route: { page: "import" }, toasts: [] });
  });

  it("small_business_must_be_answered_before_next", async () => {
    const user = userEvent.setup();
    await toSenders(user);
    const next = await screen.findByRole("button", { name: "Weiter" });
    expect(next).toBeDisabled();
    expect(next).toHaveAccessibleDescription(
      /Noch offen, ob Kleinunternehmer: Die Band, Studio Beispiel/,
    );
    await answerSmallBusiness(user);
    expect(next).toBeEnabled();
  });

  it("class_change_per_row_goes_into_plan", async () => {
    const user = userEvent.setup();
    await toSenders(user);
    await answerSmallBusiness(user);
    await user.click(screen.getByRole("button", { name: "Weiter" }));
    expect(screen.getByText(/Kunde „Club Beispiel“/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Weiter" }));
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Einordnung RE-2026-0001" }),
      "SentPaid",
    );
    await user.click(screen.getByRole("button", { name: "Weiter" }));
    await user.click(screen.getByRole("button", { name: "Übernehmen" }));
    await waitFor(() => expect(applyImport).toHaveBeenCalled());
    const [token, plan] = vi.mocked(applyImport).mock.calls[0];
    expect(token).toBe("tok");
    expect(plan.invoices[0].class).toBe("SentPaid");
    expect(plan.businesses.map((b) => b.small_business)).toEqual([true, true]);
  });

  it("review_rows_show_reason_text", async () => {
    const user = userEvent.setup();
    await toSenders(user);
    await answerSmallBusiness(user);
    await user.click(screen.getByRole("button", { name: "Weiter" }));
    await user.click(screen.getByRole("button", { name: "Weiter" }));
    const row = screen.getByRole("row", { name: /RE-2026-0002/ });
    expect(within(row).getByText("Netto + Steuer ergibt nicht Brutto")).toBeInTheDocument();
    await user.click(screen.getByRole("checkbox", { name: "Nur zu prüfende zeigen" }));
    expect(screen.queryByRole("row", { name: /RE-2026-0001/ })).toBeNull();
    expect(within(row).getByText("Netto + Steuer ergibt nicht Brutto")).toBeInTheDocument();
  });

  it("cancel_before_apply_changes_nothing", async () => {
    const user = userEvent.setup();
    await toSenders(user);
    await user.click(await screen.findByRole("button", { name: "Abbrechen" }));
    expect(cancelImport).toHaveBeenCalled();
    expect(applyImport).not.toHaveBeenCalled();
    expect(useUi.getState().route).toEqual({ page: "settings" });
  });

  it("report_shows_counts_and_warnings", async () => {
    const user = userEvent.setup();
    await toSenders(user);
    await answerSmallBusiness(user);
    for (let i = 0; i < 3; i += 1) await user.click(screen.getByRole("button", { name: "Weiter" }));
    await user.click(screen.getByRole("button", { name: "Übernehmen" }));
    expect(await screen.findByText("Übernommen")).toBeInTheDocument();
    expect(screen.getByText("1 Rechnung ausgestellt (übernommen)")).toBeInTheDocument();
    expect(screen.getByText("1 zum Prüfen markiert")).toBeInTheDocument();
    expect(
      screen.getByText("An item without a product was kept as a free line."),
    ).toBeInTheDocument();
  });

  it("report_names_each_invoice_kept_aside", async () => {
    vi.mocked(applyImport).mockResolvedValueOnce({
      businesses: 2,
      customers: 1,
      products: 0,
      drafts: 1,
      issued: 0,
      payments: 0,
      quarantined: 1,
      needs_review: 0,
      already_present: 0,
      set_aside: [{ number: "RE-2026-0002", reason: "unreadable_date" }],
    });
    const user = userEvent.setup();
    await toSenders(user);
    await answerSmallBusiness(user);
    for (let i = 0; i < 3; i += 1) await user.click(screen.getByRole("button", { name: "Weiter" }));
    await user.click(screen.getByRole("button", { name: "Übernehmen" }));
    expect(await screen.findByText("Übernommen")).toBeInTheDocument();
    expect(screen.getByText("RE-2026-0002: ein Datum war nicht lesbar")).toBeInTheDocument();
  });

  it("welcome_offers_import_when_legacy_found", async () => {
    const user = userEvent.setup();
    const status: WorkspaceStatus = {
      place: { kind: "local" },
      access: "read_write",
      holder: null,
      stale: false,
      lock_free: false,
      lock_lost: false,
      sync_error: null,
      backup_error: null,
      conflict_copies: [],
      problem: null,
      demo: false,
      setup_done: false,
    };
    useWorkspace.setState({ status });
    render(<WelcomeDialog />);
    expect(await screen.findByText("Daten aus VibeBill gefunden.")).toBeInTheDocument();
    expect(findLegacyDatabases).toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Aus VibeBill übernehmen" }));
    await waitFor(() => expect(useUi.getState().route).toEqual({ page: "import" }));
  });
});
