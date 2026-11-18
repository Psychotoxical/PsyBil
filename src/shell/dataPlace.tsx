import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { DataPlace } from "../ipc/bindings/DataPlace";
import { messageFor, toCmdError } from "../ipc/errors";
import { chooseDataPlace, type PlaceMode } from "../ipc/workspace";
import { useUi } from "../store/ui";
import { useWorkspace } from "../store/workspace";
import { Dialog } from "./Dialog";

type T = (key: string, values?: Record<string, string>) => string;

/** Asks for a folder; `null` when the dialog is cancelled. */
export async function pickFolder(t: T): Promise<string | null> {
  const picked = await open({ directory: true, title: t("Choose a shared folder") });
  return typeof picked === "string" ? picked : null;
}

/**
 * Moves the data to another place. When PsyBil data is there already, a dialog
 * offers to open it, or to open it and merge the current data in.
 * `onDone` runs after the switch succeeded.
 */
export function useChangePlace(onDone?: () => void) {
  const { t } = useTranslation();
  const [occupied, setOccupied] = useState<DataPlace | null>(null);
  const [busy, setBusy] = useState(false);

  const run = async (place: DataPlace, mode: PlaceMode = "move"): Promise<boolean> => {
    setBusy(true);
    const { toast, bump } = useUi.getState();
    try {
      const report = await chooseDataPlace(place, mode);
      setOccupied(null);
      await useWorkspace.getState().load();
      bump();
      if (report) useWorkspace.getState().setReport(report);
      toast(
        "info",
        place.kind === "local"
          ? t("The data is now on this computer only.")
          : t("The data is now in {dir}.", { dir: place.dir }),
      );
      onDone?.();
      return true;
    } catch (error) {
      const e = toCmdError(error);
      if (e.code === "target_not_empty" && mode === "move") setOccupied(place);
      else toast("error", messageFor(e, t));
      // What is open may have changed half-way; the window shows what is true.
      await useWorkspace.getState().load();
      return false;
    } finally {
      setBusy(false);
    }
  };

  const dialog = occupied && (
    <Dialog
      title={t("There is PsyBil data there already")}
      onClose={() => setOccupied(null)}
      footer={
        <>
          <button type="button" className="btn" onClick={() => setOccupied(null)}>
            {t("Cancel")}
          </button>
          <button
            type="button"
            className="btn"
            disabled={busy}
            onClick={() => void run(occupied, "open_and_merge")}
          >
            {t("Open and merge my data in")}
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={busy}
            onClick={() => void run(occupied, "open")}
          >
            {t("Open there")}
          </button>
        </>
      }
    >
      <p className="dialog-text">
        {t(
          "Opening shows the data there; your current data stays as a backup on this computer. Merging also adds your current data to it.",
        )}
      </p>
    </Dialog>
  );

  return { run, dialog, busy };
}
