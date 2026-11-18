import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { i18n } from "../i18n";
import type { MergeReport } from "../ipc/bindings/MergeReport";
import { type WorkspaceStatus, workspaceStatus } from "../ipc/workspace";
import { useUi } from "./ui";

type State = {
  status: WorkspaceStatus | null;
  /** A merge to show, from the background or from merging conflict copies. */
  report: MergeReport | null;
  setStatus: (status: WorkspaceStatus) => void;
  setReport: (report: MergeReport | null) => void;
  /** A merge from the background: a dialog when there is something to decide, else a message. */
  showMerge: (report: MergeReport) => void;
  load: () => Promise<void>;
};

export const useWorkspace = create<State>((set, get) => ({
  status: null,
  report: null,
  setStatus: (status) => {
    const before = get().status;
    set({ status });
    // Other data may be open now: pages load again.
    const moved = JSON.stringify(before?.place) !== JSON.stringify(status.place);
    if (before && (before.access !== status.access || moved)) useUi.getState().bump();
  },
  setReport: (report) => {
    set({ report });
    if (report) useUi.getState().bump();
  },
  showMerge: (report) => {
    const decide = report.changed_both.length + report.unclear.length + report.quarantined.length;
    if (decide > 0) {
      get().setReport(report);
      return;
    }
    useUi.getState().bump();
    useUi.getState().toast("info", i18n.t("Changes from another computer are here now too."));
  },
  load: async () => {
    try {
      get().setStatus(await workspaceStatus());
    } catch {
      // Outside the app there is no data place.
    }
  },
}));

/** Follows what the background reports. Returns a function that stops listening. */
export function followWorkspace(): () => void {
  const stops: (() => void)[] = [];
  let gone = false;
  const keep = (stop: () => void) => {
    if (gone) stop();
    else stops.push(stop);
  };
  try {
    listen<WorkspaceStatus>("workspace-status", (e) => useWorkspace.getState().setStatus(e.payload))
      .then(keep)
      .catch(() => {});
    listen<MergeReport>("merge-report", (e) => useWorkspace.getState().showMerge(e.payload))
      .then(keep)
      .catch(() => {});
  } catch {
    // Outside the app there are no events.
  }
  return () => {
    gone = true;
    for (const stop of stops) stop();
  };
}

/** Why nothing can be changed right now, or `null` when it can. */
export function useReadOnly(): string | null {
  const { t } = useTranslation();
  const status = useWorkspace((s) => s.status);
  if (!status) return null;
  if (status.access === "read_only")
    return t("Read only: being edited on {device}", {
      device: status.holder?.device_name ?? t("another computer"),
    });
  if (status.access === "none") return t("The data is not open.");
  return null;
}
