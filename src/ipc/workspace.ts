import type { BackupEntry } from "./bindings/BackupEntry";
import type { BackupInfo } from "./bindings/BackupInfo";
import type { DataPlace } from "./bindings/DataPlace";
import type { LockInfo } from "./bindings/LockInfo";
import type { MergeReport } from "./bindings/MergeReport";
import type { QuarantineEntry } from "./bindings/QuarantineEntry";
import { type CmdError, call } from "./errors";

/** What `workspace_cmd::status` sends: where the data is and who may write. */
export type WorkspaceStatus = {
  place: DataPlace;
  access: "read_write" | "read_only" | "none";
  holder: LockInfo | null;
  stale: boolean;
  lock_free: boolean;
  lock_lost: boolean;
  sync_error: string | null;
  backup_error: string | null;
  conflict_copies: string[];
  problem: CmdError | null;
  demo: boolean;
  setup_done: boolean;
};

export type PlaceMode = "move" | "open" | "open_and_merge";

export const workspaceStatus = () => call<WorkspaceStatus>("workspace_status");
export const reloadWorkspace = () => call<WorkspaceStatus>("reload_workspace");
export const takeOverLock = () => call<WorkspaceStatus>("take_over_lock");
export const forceTakeOverLock = () => call<WorkspaceStatus>("force_take_over_lock");
export const mergeConflictCopies = (paths: string[]) =>
  call<MergeReport[]>("merge_conflict_copies", { paths });
export const keepOtherVersion = (table: string, row: Record<string, unknown>) =>
  call<void>("keep_other_version", { table, row });
export const quarantineEntries = (ids: string[]) =>
  call<QuarantineEntry[]>("quarantine_entries", { ids });
/** Returns the merge report when `mode` merges. */
export const chooseDataPlace = (place: DataPlace, mode: PlaceMode) =>
  call<MergeReport | null>("choose_data_place", { place, mode });
export const finishSetup = () => call<void>("finish_setup");

export const backupNow = (path: string) => call<string>("backup_now", { path });
export const listBackups = () => call<BackupEntry[]>("list_backups");
export const inspectBackup = (path: string) => call<BackupInfo>("inspect_backup", { path });
/** Returns the backup of the state before. */
export const restoreBackup = (path: string) => call<string>("restore_backup", { path });
export const revealBackupsFolder = () => call<void>("reveal_backups_folder");
