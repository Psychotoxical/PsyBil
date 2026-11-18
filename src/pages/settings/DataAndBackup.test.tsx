import { open } from "@tauri-apps/plugin-dialog";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../../i18n";
import {
  chooseDataPlace,
  inspectBackup,
  listBackups,
  restoreBackup,
  type WorkspaceStatus,
  workspaceStatus,
} from "../../ipc/workspace";
import { useUi } from "../../store/ui";
import { useWorkspace } from "../../store/workspace";
import { BackupSection } from "./BackupSection";
import { DataPlaceSection } from "./DataPlaceSection";

const STATUS: WorkspaceStatus = {
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
};

const AUTO = "C:/Temp/backups/auto-2026-11-18-091500.db";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("../../ipc/workspace", () => ({
  chooseDataPlace: vi.fn(async () => null),
  workspaceStatus: vi.fn(async () => STATUS),
  listBackups: vi.fn(async () => [
    { path: AUTO, file_name: "auto-2026-11-18-091500.db", kind: "auto", size: 40960 },
  ]),
  inspectBackup: vi.fn(async () => ({
    path: AUTO,
    schema_version: 3,
    data_as_of: "2026-11-18T09:15:00.000Z",
    issued: 4,
    drafts: 1,
    businesses: 2,
  })),
  restoreBackup: vi.fn(async () => "C:/Temp/backups/before-restore-2026-11-18-100000.db"),
  backupNow: vi.fn(),
  revealBackupsFolder: vi.fn(),
}));

describe("data place and backups", () => {
  beforeEach(() => {
    setLanguage("de");
    vi.clearAllMocks();
    useWorkspace.setState({ status: STATUS, report: null });
    useUi.setState({ toasts: [] });
  });

  it("restore_asks_with_date_and_counts_first", async () => {
    const user = userEvent.setup();
    render(<BackupSection />);
    const row = await screen.findByRole("listitem");
    expect(row).toHaveTextContent("Automatisch");
    expect(row).toHaveTextContent("18.11.2026");
    await user.click(within(row).getByRole("button", { name: "Wiederherstellen …" }));
    expect(inspectBackup).toHaveBeenCalledWith(AUTO);
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Stand vom 18.11.2026");
    expect(dialog).toHaveTextContent("4 Belege, 1 Entwurf");
    expect(dialog).toHaveTextContent("Der jetzige Stand wird vorher gesichert.");
    expect(restoreBackup).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("button", { name: "Wiederherstellen" }));
    await waitFor(() => expect(restoreBackup).toHaveBeenCalledWith(AUTO));
  });

  it("restore_cancel_calls_nothing", async () => {
    const user = userEvent.setup();
    render(<BackupSection />);
    const row = await screen.findByRole("listitem");
    await user.click(within(row).getByRole("button", { name: "Wiederherstellen …" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Abbrechen" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(restoreBackup).not.toHaveBeenCalled();
  });

  it("damaged_backup_is_refused_with_a_message", async () => {
    const user = userEvent.setup();
    vi.mocked(inspectBackup).mockRejectedValueOnce({
      code: "backup_damaged",
      message: "malformed",
      issues: [],
    });
    render(<BackupSection />);
    const row = await screen.findByRole("listitem");
    await user.click(within(row).getByRole("button", { name: "Wiederherstellen …" }));
    await waitFor(() =>
      expect(useUi.getState().toasts[0]?.text).toBe(
        "Die Sicherung ist beschädigt und lässt sich nicht wiederherstellen.",
      ),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("data_place_change_to_occupied_folder_offers_open_or_merge", async () => {
    const user = userEvent.setup();
    vi.mocked(open).mockResolvedValueOnce("C:/Temp/Nextcloud/PsyBil");
    vi.mocked(chooseDataPlace).mockRejectedValueOnce({
      code: "target_not_empty",
      message: "target holds data",
      issues: [],
    });
    render(<DataPlaceSection />);
    expect(screen.getByText("Nur auf diesem Rechner")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Gemeinsamen Ordner wählen …" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Dort liegen schon PsyBil-Daten");
    expect(within(dialog).getByRole("button", { name: "Dort öffnen" })).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Öffnen und zusammenführen" }));
    await waitFor(() =>
      expect(chooseDataPlace).toHaveBeenLastCalledWith(
        { kind: "shared", dir: "C:/Temp/Nextcloud/PsyBil" },
        "open_and_merge",
      ),
    );
  });

  it("failed_place_change_reloads_the_status", async () => {
    const user = userEvent.setup();
    vi.mocked(open).mockResolvedValueOnce("C:/Temp/Nextcloud/PsyBil");
    vi.mocked(chooseDataPlace).mockRejectedValueOnce({
      code: "lock_held",
      message: "held",
      issues: [],
    });
    render(<DataPlaceSection />);
    await user.click(screen.getByRole("button", { name: "Gemeinsamen Ordner wählen …" }));
    await waitFor(() => expect(workspaceStatus).toHaveBeenCalled());
    expect(useUi.getState().toasts[0]?.text).toBe(
      "Ein anderer Rechner bearbeitet die Daten gerade.",
    );
  });

  it("no_backups_yet_says_so", async () => {
    vi.mocked(listBackups).mockResolvedValueOnce([]);
    render(<BackupSection />);
    expect(await screen.findByText("Noch keine Sicherung vorhanden.")).toBeInTheDocument();
  });

  it("demo_keeps_the_data_place", () => {
    useWorkspace.setState({ status: { ...STATUS, demo: true } });
    render(<DataPlaceSection />);
    expect(screen.getByText("Im Beispielmodus bleibt der Datenordner fest.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Gemeinsamen Ordner wählen …" })).toBeNull();
  });
});
