import { open } from "@tauri-apps/plugin-dialog";
import { ArchiveRestore, FolderOpen, Save } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatDate, formatDateTime } from "../../format";
import type { BackupEntry } from "../../ipc/bindings/BackupEntry";
import type { BackupInfo } from "../../ipc/bindings/BackupInfo";
import type { BackupKind } from "../../ipc/bindings/BackupKind";
import { messageFor, toCmdError } from "../../ipc/errors";
import {
  backupNow,
  inspectBackup,
  listBackups,
  restoreBackup,
  revealBackupsFolder,
} from "../../ipc/workspace";
import { ConfirmDialog } from "../../shell/ConfirmDialog";
import { saveExport } from "../../shell/saveExport";
import { useUi } from "../../store/ui";
import { useWorkspace } from "../../store/workspace";

/** "auto-2026-11-18-091500.db" → "18.11.2026, 09:15". */
function stampOf(name: string, lang: string): string {
  const m = /(\d{4})-(\d{2})-(\d{2})-(\d{2})(\d{2})\d{2}\.db$/.exec(name);
  if (!m) return name;
  return `${formatDate(`${m[1]}-${m[2]}-${m[3]}`, lang)}, ${m[4]}:${m[5]}`;
}

const pad = (n: number) => String(n).padStart(2, "0");

/** The name "Back up now" suggests: psybil-backup-2026-11-18-143005.db. */
function backupName(): string {
  const d = new Date();
  return `psybil-backup-${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}.db`;
}

/** Backing up by hand, the daily backups, and restoring one. */
export function BackupSection() {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const toast = useUi((s) => s.toast);
  const readOnly = useWorkspace((s) => s.status?.access !== "read_write");
  const [entries, setEntries] = useState<BackupEntry[]>([]);
  const [pending, setPending] = useState<{ path: string; info: BackupInfo } | null>(null);
  const [busy, setBusy] = useState(false);

  const kinds: Record<BackupKind, string> = {
    auto: t("Automatic"),
    manual: t("By hand"),
    before_restore: t("Before restoring"),
    before_import: t("Before the import"),
    before_move: t("Before the move"),
  };

  const load = useCallback(() => {
    listBackups()
      .then(setEntries)
      .catch(() => setEntries([]));
  }, []);
  useEffect(load, [load]);

  const fail = (error: unknown) => toast("error", messageFor(toCmdError(error), t));

  const ask = async (path: string) => {
    try {
      setPending({ path, info: await inspectBackup(path) });
    } catch (error) {
      fail(error);
    }
  };

  const restore = async (path: string) => {
    setPending(null);
    setBusy(true);
    try {
      await restoreBackup(path);
      toast("info", t("Restored. The state before is kept as a backup."));
      await useWorkspace.getState().load();
      useUi.getState().bump();
      load();
    } catch (error) {
      fail(error);
    } finally {
      setBusy(false);
    }
  };

  const pickFile = async () => {
    const picked = await open({ filters: [{ name: "PsyBil", extensions: ["db"] }] });
    if (typeof picked === "string") await ask(picked);
  };

  return (
    <section className="form-section">
      <h2 className="settings-title">{t("Backup")}</h2>
      <p className="field-hint">
        {t("PsyBil backs up once a day at start and keeps the last 10 automatic backups.")}
      </p>
      <div className="settings-actions">
        <button
          type="button"
          className="btn"
          onClick={() =>
            void saveExport(t, {
              name: backupName(),
              type: "PsyBil",
              extension: "db",
              write: backupNow,
            })
          }
        >
          <Save size={16} aria-hidden="true" />
          {t("Back up now…")}
        </button>
        <button
          type="button"
          className="btn"
          disabled={readOnly || busy}
          onClick={() => void pickFile()}
        >
          <ArchiveRestore size={16} aria-hidden="true" />
          {t("Restore from file…")}
        </button>
        <button
          type="button"
          className="btn ghost"
          onClick={() => {
            revealBackupsFolder().catch(fail);
          }}
        >
          <FolderOpen size={16} aria-hidden="true" />
          {t("Show folder")}
        </button>
      </div>
      {entries.length === 0 && <p className="field-hint">{t("No backup yet.")}</p>}
      {entries.length > 0 && (
        <ul className="backup-list">
          {entries.map((e) => (
            <li key={e.path}>
              <span className="backup-kind">{kinds[e.kind]}</span>
              <span className="num">{stampOf(e.file_name, lang)}</span>
              <button
                type="button"
                className="btn ghost"
                disabled={readOnly || busy}
                onClick={() => void ask(e.path)}
              >
                {t("Restore…")}
              </button>
            </li>
          ))}
        </ul>
      )}
      {pending && (
        <ConfirmDialog
          title={t("Restore backup?")}
          text={t(
            "Backup with data as of {date}: {documents}, {drafts}. The current state is backed up first.",
            {
              date: pending.info.data_as_of
                ? formatDateTime(pending.info.data_as_of, lang)
                : t("unknown"),
              documents:
                pending.info.issued === 1
                  ? t("1 document")
                  : t("{n} documents", { n: String(pending.info.issued) }),
              drafts:
                pending.info.drafts === 1
                  ? t("1 draft")
                  : t("{n} drafts", { n: String(pending.info.drafts) }),
            },
          )}
          confirmLabel={t("Restore backup")}
          onCancel={() => setPending(null)}
          onConfirm={() => void restore(pending.path)}
        />
      )}
    </section>
  );
}
