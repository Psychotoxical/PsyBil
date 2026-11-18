import { CircleAlert, CircleCheck } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ChangedBoth } from "../ipc/bindings/ChangedBoth";
import type { MergeReport } from "../ipc/bindings/MergeReport";
import type { QuarantineEntry } from "../ipc/bindings/QuarantineEntry";
import { messageFor, toCmdError } from "../ipc/errors";
import { keepOtherVersion, mergeConflictCopies, quarantineEntries } from "../ipc/workspace";
import { useUi } from "../store/ui";
import { useWorkspace } from "../store/workspace";
import { Dialog } from "./Dialog";
import { fileNameOf } from "./saveExport";

/** Several reports as one. */
function combine(reports: MergeReport[]): MergeReport {
  return reports.reduce(
    (all, r) => ({
      added: all.added + r.added,
      updated: all.updated + r.updated,
      deleted: all.deleted + r.deleted,
      changed_both: [...all.changed_both, ...r.changed_both],
      unclear: [...all.unclear, ...r.unclear],
      quarantined: [...all.quarantined, ...r.quarantined],
      skipped_deletes: [...all.skipped_deletes, ...r.skipped_deletes],
      restored: [...all.restored, ...r.restored],
    }),
    {
      added: 0,
      updated: 0,
      deleted: 0,
      changed_both: [],
      unclear: [],
      quarantined: [],
      skipped_deletes: [],
      restored: [],
    },
  );
}

function Report({ report, onClose }: { report: MergeReport; onClose: () => void }) {
  const { t } = useTranslation();
  const toast = useUi((s) => s.toast);
  const navigate = useUi((s) => s.navigate);
  const [taken, setTaken] = useState<Set<string>>(new Set());
  const [entries, setEntries] = useState<QuarantineEntry[]>([]);
  const both = [...report.changed_both, ...report.unclear];
  const reasons: Record<string, string> = {
    issued_on_both_sides: t("issued on both computers"),
    cancelled_on_both_sides: t("cancelled on both computers"),
    depends_on_quarantined: t("belongs to a document that was put aside"),
    number_collision: t("number used twice"),
  };

  useEffect(() => {
    if (report.quarantined.length === 0) return;
    quarantineEntries(report.quarantined)
      .then(setEntries)
      .catch(() => {});
  }, [report.quarantined]);

  const takeOther = async (row: ChangedBoth) => {
    if (!row.other) return;
    try {
      await keepOtherVersion(row.table, row.other);
      setTaken(new Set([...taken, row.id]));
      useUi.getState().bump();
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t));
    }
  };

  return (
    <Dialog
      title={t("Merged")}
      wide
      onClose={onClose}
      footer={
        <button type="button" className="btn primary" onClick={onClose}>
          {t("Done")}
        </button>
      }
    >
      <p className="dialog-text">
        {t("{added} added, {updated} changed, {deleted} deleted", {
          added: String(report.added),
          updated: String(report.updated),
          deleted: String(report.deleted),
        })}
      </p>
      {both.length > 0 && (
        <section className="merge-section">
          <h3>{t("Changed on both computers")}</h3>
          <p className="field-hint">
            {t("PsyBil kept the newer version. You can take the other one instead.")}
          </p>
          <ul className="merge-list">
            {both.map((row) => (
              <li key={`${row.table}-${row.id}`}>
                <span className="merge-label">{row.label || row.table}</span>
                {taken.has(row.id) ? (
                  <span className="badge level ok">
                    <CircleCheck size={14} aria-hidden="true" />
                    {t("Other version taken")}
                  </span>
                ) : row.other ? (
                  <button type="button" className="btn" onClick={() => void takeOther(row)}>
                    {t("Take the other version")}
                  </button>
                ) : row.table === "invoices" ? (
                  <button
                    type="button"
                    className="btn"
                    onClick={() => {
                      onClose();
                      navigate({ page: "assistant", id: row.id });
                    }}
                  >
                    {t("Open draft")}
                  </button>
                ) : null}
              </li>
            ))}
          </ul>
        </section>
      )}
      {entries.length > 0 && (
        <section className="merge-section">
          <h3>{t("Needs your attention")}</h3>
          <ul className="merge-list warn">
            {entries.map((e) => (
              <li key={e.id}>
                <CircleAlert size={16} aria-hidden="true" />
                <span>
                  {e.reason === "number_collision" && e.number
                    ? t(
                        "The number {number} exists twice. Cancel the newer invoice and issue it again.",
                        { number: e.number },
                      )
                    : t("A document could not be placed ({reason}). It is kept aside.", {
                        reason: reasons[e.reason] ?? e.reason,
                      })}
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}
    </Dialog>
  );
}

/**
 * Offers to merge cloud conflict copies of the shared file, and shows what a
 * merge did when there is something to decide.
 */
export function MergeDialog() {
  const { t } = useTranslation();
  const status = useWorkspace((s) => s.status);
  const report = useWorkspace((s) => s.report);
  const setReport = useWorkspace((s) => s.setReport);
  const toast = useUi((s) => s.toast);
  const [later, setLater] = useState(false);
  const [busy, setBusy] = useState(false);
  const copies = status?.access === "read_write" ? status.conflict_copies : [];

  if (report) return <Report report={report} onClose={() => setReport(null)} />;
  if (copies.length === 0 || later) return null;

  const merge = async () => {
    setBusy(true);
    try {
      setReport(combine(await mergeConflictCopies(copies)));
      await useWorkspace.getState().load();
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      title={t("Conflict copy found")}
      onClose={() => setLater(true)}
      footer={
        <>
          <button type="button" className="btn" onClick={() => setLater(true)}>
            {t("Later")}
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={busy}
            onClick={() => void merge()}
          >
            {t("Merge")}
          </button>
        </>
      }
    >
      <p className="dialog-text">
        {t(
          "The data folder holds a conflict copy made by the cloud sync: {names}. PsyBil can merge it with your data; the copy is then put aside.",
          { names: copies.map(fileNameOf).join(", ") },
        )}
      </p>
    </Dialog>
  );
}
