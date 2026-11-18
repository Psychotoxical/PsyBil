import { FolderOpen, HardDrive, RotateCw } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { messageFor, toCmdError } from "../ipc/errors";
import { reloadWorkspace } from "../ipc/workspace";
import { useUi } from "../store/ui";
import { useWorkspace } from "../store/workspace";
import { pickFolder, useChangePlace } from "./dataPlace";

/** Instead of the page when the data cannot be opened, e.g. a shared folder that is not there. */
export function ProblemPage() {
  const { t } = useTranslation();
  const status = useWorkspace((s) => s.status);
  const toast = useUi((s) => s.toast);
  const { run, dialog, busy } = useChangePlace();
  const [retrying, setRetrying] = useState(false);
  if (!status) return null;
  const dir = status.place.kind === "shared" ? status.place.dir : null;

  const retry = async () => {
    setRetrying(true);
    try {
      useWorkspace.getState().setStatus(await reloadWorkspace());
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t));
    } finally {
      setRetrying(false);
    }
  };

  return (
    <div className="empty problem">
      <h2>{dir ? t("The data folder cannot be reached") : t("The data cannot be opened")}</h2>
      {dir && <p className="num problem-path">{dir}</p>}
      {status.problem && <p>{messageFor(status.problem, t)}</p>}
      {dir && <p>{t("Is the folder there, for example is the cloud drive running?")}</p>}
      <div className="problem-actions">
        <button
          type="button"
          className="btn primary"
          disabled={retrying || busy}
          onClick={() => void retry()}
        >
          <RotateCw size={16} aria-hidden="true" />
          {t("Try again")}
        </button>
        <button
          type="button"
          className="btn"
          disabled={busy}
          onClick={async () => {
            const picked = await pickFolder(t);
            if (picked) await run({ kind: "shared", dir: picked }, "open");
          }}
        >
          <FolderOpen size={16} aria-hidden="true" />
          {t("Choose another folder…")}
        </button>
        <button
          type="button"
          className="btn"
          disabled={busy}
          onClick={() => void run({ kind: "local" }, "open")}
        >
          <HardDrive size={16} aria-hidden="true" />
          {t("Work on this computer only")}
        </button>
      </div>
      {dialog}
    </div>
  );
}
