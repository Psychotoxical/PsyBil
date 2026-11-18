import { FolderSync, HardDrive } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { messageFor, toCmdError } from "../ipc/errors";
import { findLegacyDatabases } from "../ipc/legacy";
import { finishSetup } from "../ipc/workspace";
import { useUi } from "../store/ui";
import { useWorkspace } from "../store/workspace";
import { Dialog } from "./Dialog";
import { pickFolder, useChangePlace } from "./dataPlace";

/** The first start: where the data lives. Asked once per computer. */
export function WelcomeDialog() {
  const { t } = useTranslation();
  const status = useWorkspace((s) => s.status);
  const toast = useUi((s) => s.toast);
  const [done, setDone] = useState(false);
  const [legacy, setLegacy] = useState(false);
  const asking = status !== null && !status.setup_done && !status.demo;

  useEffect(() => {
    if (!asking) return;
    findLegacyDatabases()
      .then((found) => setLegacy(found.length > 0))
      .catch(() => setLegacy(false));
  }, [asking]);

  const finish = async () => {
    try {
      await finishSetup();
      setDone(true);
      await useWorkspace.getState().load();
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t));
    }
  };
  const { run, dialog, busy } = useChangePlace(() => void finish());

  if (!status || status.setup_done || status.demo || status.access === "none" || done) return null;

  return (
    <>
      {/* Closing without an answer asks again at the next start. */}
      <Dialog title={t("Welcome to PsyBil")} onClose={() => setDone(true)}>
        <p className="dialog-text">
          {t("Where should your data live? You can change this later in the settings.")}
        </p>
        <div className="answers welcome-answers">
          <button type="button" className="answer" disabled={busy} onClick={() => void finish()}>
            <span className="answer-icon">
              <HardDrive size={22} aria-hidden="true" />
            </span>
            <span className="answer-title">{t("On this computer only")}</span>
            <span className="answer-sub">
              {t("The data stays in PsyBil's folder on this computer.")}
            </span>
          </button>
          <button
            type="button"
            className="answer"
            disabled={busy}
            onClick={async () => {
              const dir = await pickFolder(t);
              if (dir) await run({ kind: "shared", dir });
            }}
          >
            <span className="answer-icon">
              <FolderSync size={22} aria-hidden="true" />
            </span>
            <span className="answer-title">{t("In a shared folder")}</span>
            <span className="answer-sub">
              {t("For example in a Nextcloud, so several computers use the same data.")}
            </span>
          </button>
        </div>
        {legacy && (
          <div className="welcome-legacy">
            <p>{t("VibeBill data found.")}</p>
            <button
              type="button"
              className="btn"
              onClick={async () => {
                await finish();
                useUi.getState().navigate({ page: "import" });
              }}
            >
              {t("Take over from VibeBill")}
            </button>
          </div>
        )}
      </Dialog>
      {dialog}
    </>
  );
}
