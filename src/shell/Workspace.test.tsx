import { open } from "@tauri-apps/plugin-dialog";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../i18n";
import type { MergeReport } from "../ipc/bindings/MergeReport";
import {
  chooseDataPlace,
  finishSetup,
  forceTakeOverLock,
  keepOtherVersion,
  mergeConflictCopies,
  quarantineEntries,
  reloadWorkspace,
  takeOverLock,
  type WorkspaceStatus,
} from "../ipc/workspace";
import { useUi } from "../store/ui";
import { useWorkspace } from "../store/workspace";
import { FormBar } from "../ui/FormBar";
import { LockBanner } from "./LockBanner";
import { MergeDialog } from "./MergeDialog";
import { ProblemPage } from "./ProblemPage";
import { Sidebar } from "./Sidebar";
import { WelcomeDialog } from "./WelcomeDialog";

const BASE: WorkspaceStatus = {
  place: { kind: "shared", dir: "C:/Temp/Nextcloud/PsyBil" },
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
};

const HOLDER = {
  device_id: "d2",
  device_name: "Mac",
  user: "band",
  started: "2026-11-18T09:05:00Z",
  heartbeat: "2026-11-18T09:40:00Z",
};

const status = (o: Partial<WorkspaceStatus>) =>
  useWorkspace.getState().setStatus({ ...BASE, ...o });

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("../ipc/workspace", () => ({
  reloadWorkspace: vi.fn(async () => BASE),
  takeOverLock: vi.fn(async () => BASE),
  forceTakeOverLock: vi.fn(async () => BASE),
  mergeConflictCopies: vi.fn(async () => []),
  keepOtherVersion: vi.fn(async () => {}),
  quarantineEntries: vi.fn(async () => []),
  chooseDataPlace: vi.fn(async () => null),
  finishSetup: vi.fn(async () => {}),
  workspaceStatus: vi.fn(async () => BASE),
}));
vi.mock("../ipc/masterdata", () => ({ listBusinesses: vi.fn(async () => []) }));

const REPORT: MergeReport = {
  added: 2,
  updated: 1,
  deleted: 0,
  changed_both: [
    {
      table: "customers",
      id: "c1",
      kept: "Other",
      label: "Club Beispiel",
      other: { id: "c1", name: "Club Beispiel", city: "Musterstadt" },
    },
    { table: "invoices", id: "d1", kept: "Local", label: "2026-11-02", other: null },
  ],
  unclear: [],
  quarantined: ["q1"],
  skipped_deletes: [],
  restored: [],
};

describe("workspace in the window", () => {
  beforeEach(() => {
    setLanguage("de");
    vi.clearAllMocks();
    useWorkspace.setState({ status: null, report: null });
    useUi.setState({ route: { page: "overview" }, toasts: [] });
  });

  it("banner_shows_device_and_since_for_foreign_lock", async () => {
    const user = userEvent.setup();
    status({ access: "read_only", holder: HOLDER });
    render(<LockBanner />);
    const banner = screen.getByRole("status");
    expect(banner).toHaveTextContent("Wird gerade auf Mac bearbeitet (seit 18.11.2026");
    expect(banner).toHaveTextContent("Du kannst nur lesen.");
    await user.click(within(banner).getByRole("button", { name: "Neu laden" }));
    expect(reloadWorkspace).toHaveBeenCalled();
  });

  it("force_take_over_needs_confirmation", async () => {
    const user = userEvent.setup();
    status({ access: "read_only", holder: HOLDER });
    render(<LockBanner />);
    await user.click(screen.getByRole("button", { name: "Trotzdem übernehmen …" }));
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent("Auf Mac lässt sich dann nicht mehr speichern.");
    expect(forceTakeOverLock).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("button", { name: "Trotzdem übernehmen" }));
    await waitFor(() => expect(forceTakeOverLock).toHaveBeenCalled());
  });

  it("stale_lock_banner_offers_take_over", async () => {
    const user = userEvent.setup();
    status({ access: "read_only", holder: HOLDER, stale: true });
    render(<LockBanner />);
    expect(screen.getByRole("status")).toHaveTextContent("Mac hat sich seit 18.11.2026");
    await user.click(screen.getByRole("button", { name: "Übernehmen" }));
    expect(takeOverLock).toHaveBeenCalled();
    expect(forceTakeOverLock).not.toHaveBeenCalled();
  });

  it("free_lock_banner_reads_right_without_a_holder", () => {
    status({ access: "read_only", holder: null, lock_free: true });
    render(<LockBanner />);
    expect(screen.getByRole("status")).toHaveTextContent(
      "Der andere Rechner hat PsyBil geschlossen. Neu laden, um zu bearbeiten.",
    );
  });

  it("merge_dialog_names_reasons_in_words", async () => {
    vi.mocked(quarantineEntries).mockResolvedValueOnce([
      {
        id: "q2",
        kind: "invoice",
        reason: "issued_on_both_sides",
        number: null,
        business_id: null,
      },
    ]);
    useWorkspace.getState().setReport({ ...REPORT, changed_both: [], quarantined: ["q2"] });
    render(<MergeDialog />);
    expect(await screen.findByText(/auf beiden Rechnern ausgestellt/)).toBeInTheDocument();
    expect(screen.queryByText(/issued_on_both_sides/)).toBeNull();
  });

  it("merge_dialog_names_unreadable_date", async () => {
    vi.mocked(quarantineEntries).mockResolvedValueOnce([
      {
        id: "q3",
        kind: "legacy_invoice",
        reason: "unreadable_date",
        number: null,
        business_id: null,
      },
    ]);
    useWorkspace.getState().setReport({ ...REPORT, changed_both: [], quarantined: ["q3"] });
    render(<MergeDialog />);
    expect(await screen.findByText(/ein Datum war nicht lesbar/)).toBeInTheDocument();
    expect(screen.queryByText(/unreadable_date/)).toBeNull();
  });

  it("read_only_disables_new_invoice_and_save_with_reason", async () => {
    status({ access: "read_only", holder: HOLDER });
    render(
      <>
        <Sidebar />
        <FormBar formId="f" missing={[]} busy={false} dirty onDiscard={() => {}} />
      </>,
    );
    const reason = "Nur lesen: wird auf Mac bearbeitet";
    const newInvoice = screen.getByRole("button", { name: /Neue Rechnung/ });
    expect(newInvoice).toBeDisabled();
    expect(newInvoice).toHaveAccessibleDescription(reason);
    const saveButton = screen.getByRole("button", { name: "Speichern" });
    expect(saveButton).toBeDisabled();
    expect(saveButton).toHaveAccessibleDescription(reason);
  });

  it("problem_page_offers_retry_and_local", async () => {
    const user = userEvent.setup();
    status({
      access: "none",
      problem: { code: "io", message: "folder not found", issues: [] },
    });
    render(<ProblemPage />);
    expect(screen.getByRole("heading")).toHaveTextContent("Der Datenordner ist nicht erreichbar");
    expect(screen.getByText("C:/Temp/Nextcloud/PsyBil")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Erneut versuchen" }));
    expect(reloadWorkspace).toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Nur auf diesem Rechner arbeiten" }));
    await waitFor(() => expect(chooseDataPlace).toHaveBeenCalledWith({ kind: "local" }, "open"));
  });

  it("merge_dialog_lists_changed_both_and_duplicates", async () => {
    vi.mocked(quarantineEntries).mockResolvedValueOnce([
      {
        id: "q1",
        kind: "invoice",
        reason: "number_collision",
        number: "RE-2026-0003",
        business_id: "u1",
      },
    ]);
    useWorkspace.getState().setReport(REPORT);
    render(<MergeDialog />);
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent("2 hinzugekommen, 1 geändert, 0 gelöscht");
    expect(within(dialog).getByText("Club Beispiel")).toBeInTheDocument();
    expect(
      await within(dialog).findByText(
        "Die Nummer RE-2026-0003 gibt es zweimal. Storniere die jüngere Rechnung und stelle sie neu aus.",
      ),
    ).toBeInTheDocument();
    expect(quarantineEntries).toHaveBeenCalledWith(["q1"]);
  });

  it("keep_other_version_button_only_with_row", async () => {
    const user = userEvent.setup();
    useWorkspace.getState().setReport(REPORT);
    render(<MergeDialog />);
    const dialog = screen.getByRole("dialog");
    const buttons = within(dialog).getAllByRole("button", { name: "Anderen Stand nehmen" });
    expect(buttons).toHaveLength(1);
    await user.click(buttons[0]);
    expect(keepOtherVersion).toHaveBeenCalledWith("customers", REPORT.changed_both[0].other);
    expect(await within(dialog).findByText("Anderer Stand übernommen")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Entwurf öffnen" }));
    expect(useUi.getState().route).toEqual({ page: "assistant", id: "d1" });
  });

  it("conflict_copy_is_offered_for_merging", async () => {
    const user = userEvent.setup();
    vi.mocked(mergeConflictCopies).mockResolvedValueOnce([REPORT]);
    status({ conflict_copies: ["C:/Temp/Nextcloud/PsyBil/psybil (conflicted copy).db"] });
    render(<MergeDialog />);
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent("psybil (conflicted copy).db");
    await user.click(within(dialog).getByRole("button", { name: "Zusammenführen" }));
    expect(mergeConflictCopies).toHaveBeenCalledWith([
      "C:/Temp/Nextcloud/PsyBil/psybil (conflicted copy).db",
    ]);
    expect(await screen.findByText("2 hinzugekommen, 1 geändert, 0 gelöscht")).toBeInTheDocument();
  });

  it("quiet_background_merge_only_tells", () => {
    useWorkspace.getState().showMerge({ ...REPORT, changed_both: [], quarantined: [] });
    expect(useWorkspace.getState().report).toBeNull();
    expect(useUi.getState().toasts[0]?.text).toBe(
      "Änderungen eines anderen Rechners sind jetzt auch hier.",
    );
    useWorkspace.getState().showMerge(REPORT);
    expect(useWorkspace.getState().report).toEqual(REPORT);
  });

  it("welcome_once_then_setup_done", async () => {
    const user = userEvent.setup();
    status({ place: { kind: "local" }, setup_done: false });
    const { unmount } = render(<WelcomeDialog />);
    const dialog = screen.getByRole("dialog", { name: "Willkommen bei PsyBil" });
    await user.click(within(dialog).getByRole("button", { name: /Nur auf diesem Rechner/ }));
    await waitFor(() => expect(finishSetup).toHaveBeenCalled());
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    unmount();
    status({ place: { kind: "local" }, setup_done: true });
    render(<WelcomeDialog />);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("closing_welcome_asks_again_later", async () => {
    const user = userEvent.setup();
    status({ place: { kind: "local" }, setup_done: false });
    render(<WelcomeDialog />);
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(finishSetup).not.toHaveBeenCalled();
  });

  it("welcome_shared_folder_moves_data_there", async () => {
    const user = userEvent.setup();
    vi.mocked(open).mockResolvedValueOnce("C:/Temp/Nextcloud/PsyBil");
    status({ place: { kind: "local" }, setup_done: false });
    render(<WelcomeDialog />);
    await user.click(screen.getByRole("button", { name: /gemeinsamen Ordner/ }));
    await waitFor(() =>
      expect(chooseDataPlace).toHaveBeenCalledWith(
        { kind: "shared", dir: "C:/Temp/Nextcloud/PsyBil" },
        "move",
      ),
    );
    await waitFor(() => expect(finishSetup).toHaveBeenCalled());
  });
});
