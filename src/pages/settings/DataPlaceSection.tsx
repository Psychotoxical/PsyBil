import { FolderSync, HardDrive } from "lucide-react";
import { useTranslation } from "react-i18next";
import { pickFolder, useChangePlace } from "../../shell/dataPlace";
import { useWorkspace } from "../../store/workspace";

/** Where the data lives on this computer, and moving it. */
export function DataPlaceSection() {
  const { t } = useTranslation();
  const status = useWorkspace((s) => s.status);
  const { run, dialog, busy } = useChangePlace();
  const place = status?.place ?? { kind: "local" as const };
  // While another computer holds the lock, this copy is not the newest state.
  const readOnly = status?.access === "read_only";
  const locked = busy || readOnly;
  const describedBy = readOnly ? "data-place-locked" : undefined;

  const chooseShared = async () => {
    const dir = await pickFolder(t);
    if (dir) await run({ kind: "shared", dir });
  };

  return (
    <section className="form-section">
      <h2 className="settings-title">{t("Data folder")}</h2>
      <div className="data-place">
        {place.kind === "local" ? (
          <>
            <HardDrive size={18} aria-hidden="true" />
            <span>{t("On this computer only")}</span>
          </>
        ) : (
          <>
            <FolderSync size={18} aria-hidden="true" />
            <span>
              {t("Shared folder")} <span className="num data-place-dir">{place.dir}</span>
            </span>
          </>
        )}
      </div>
      {status?.demo ? (
        <p className="field-hint">{t("In demo mode the data folder stays fixed.")}</p>
      ) : (
        <>
          <p className="field-hint">
            {t(
              "In a shared folder several computers use the same data; while one writes, the others can read. Backups always stay on this computer.",
            )}
          </p>
          {readOnly && (
            <p className="field-hint" id="data-place-locked">
              {t(
                "While another computer is working with the data, the data folder cannot be changed.",
              )}
            </p>
          )}
          <div className="settings-actions">
            {place.kind === "local" ? (
              <button
                type="button"
                className="btn"
                disabled={locked}
                aria-describedby={describedBy}
                onClick={() => void chooseShared()}
              >
                {t("Choose a shared folder…")}
              </button>
            ) : (
              <>
                <button
                  type="button"
                  className="btn"
                  disabled={locked}
                  aria-describedby={describedBy}
                  onClick={() => void chooseShared()}
                >
                  {t("Choose another folder…")}
                </button>
                <button
                  type="button"
                  className="btn"
                  disabled={locked}
                  aria-describedby={describedBy}
                  onClick={() => void run({ kind: "local" })}
                >
                  {t("Back to this computer only")}
                </button>
              </>
            )}
          </div>
        </>
      )}
      {dialog}
    </section>
  );
}
