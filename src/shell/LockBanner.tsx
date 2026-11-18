import { CircleAlert, Lock, LockOpen } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { formatDateTime } from "../format";
import { messageFor, toCmdError } from "../ipc/errors";
import {
  forceTakeOverLock,
  reloadWorkspace,
  takeOverLock,
  type WorkspaceStatus,
} from "../ipc/workspace";
import { useUi } from "../store/ui";
import { useWorkspace } from "../store/workspace";
import { marks, rich } from "../ui/rich";
import { ConfirmDialog } from "./ConfirmDialog";

/** Above the page while another computer holds the shared folder, or writing back fails. */
export function LockBanner() {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const status = useWorkspace((s) => s.status);
  const toast = useUi((s) => s.toast);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);

  if (!status) return null;
  const reading = status.access === "read_only";
  if (!reading && !status.sync_error) return null;

  const act = async (step: () => Promise<WorkspaceStatus>) => {
    setBusy(true);
    try {
      useWorkspace.getState().setStatus(await step());
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t));
    } finally {
      setBusy(false);
    }
  };

  const device = status.holder?.device_name ?? t("another computer");
  const button = (label: string, step: () => Promise<WorkspaceStatus>, primary = false) => (
    <button
      type="button"
      className={primary ? "btn primary" : "btn"}
      disabled={busy}
      onClick={() => void act(step)}
    >
      {label}
    </button>
  );

  const at = (iso: string | undefined) => (
    <span className="num">{iso ? formatDateTime(iso, lang) : "?"}</span>
  );
  let line: React.ReactNode = null;
  let actions: React.ReactNode = null;
  let Icon = Lock;
  if (reading && status.lock_free) {
    Icon = LockOpen;
    // A free lock has no holder any more, so the sentence names none.
    line = t("The other computer has closed PsyBil. Reload to edit.");
    actions = button(t("Reload"), reloadWorkspace, true);
  } else if (reading && status.stale) {
    line = rich(t("{device} has not reported since {time}.", marks("device", "time")), {
      device,
      time: at(status.holder?.heartbeat),
    });
    actions = button(t("Take over"), takeOverLock, true);
  } else if (reading) {
    line = rich(
      t("Being edited on {device} (since {time}). You can only read.", marks("device", "time")),
      { device, time: at(status.holder?.started) },
    );
    actions = (
      <>
        {button(t("Reload"), reloadWorkspace)}
        <button type="button" className="btn" disabled={busy} onClick={() => setConfirm(true)}>
          {t("Take over anyway…")}
        </button>
      </>
    );
  }

  return (
    <div className="lock-banner" role="status">
      <div className="lock-banner-text">
        {line && (
          <p>
            <Icon size={16} aria-hidden="true" />
            <span>{line}</span>
          </p>
        )}
        {reading && status.lock_lost && (
          <p className="lock-banner-note">
            {t(
              "Another computer took over. Your last changes stay on this computer and are merged later.",
            )}
          </p>
        )}
        {status.sync_error && (
          <p>
            <CircleAlert size={16} aria-hidden="true" />
            {t("Writing back to the shared folder failed: {detail}. PsyBil keeps trying.", {
              detail: status.sync_error,
            })}
          </p>
        )}
      </div>
      {actions && <div className="lock-banner-actions">{actions}</div>}
      {confirm && (
        <ConfirmDialog
          title={t("Take over anyway?")}
          text={t(
            "On {device} nothing can be saved any more. What is not written back there yet is added only when that computer takes over again.",
            { device },
          )}
          confirmLabel={t("Take over anyway")}
          danger
          onCancel={() => setConfirm(false)}
          onConfirm={() => {
            setConfirm(false);
            void act(forceTakeOverLock);
          }}
        />
      )}
    </div>
  );
}
