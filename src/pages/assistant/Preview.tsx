import { CircleAlert, CircleCheck, LoaderCircle } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Issue } from "../../ipc/bindings/Issue";
import { messageFor, toCmdError } from "../../ipc/errors";
import { checkIssue } from "../../ipc/invoices";
import { issueText } from "../../ipc/issueTexts";
import { previewPng } from "../../ipc/pdf";
import { useDraft } from "../../store/draft";

/** The preview waits this long after a save before it asks for new pages. */
export const PREVIEW_DELAY = 500;
/** Twice the panel width, so the page stays sharp. */
const WIDTH_PX = 800;

type Props = {
  id: string;
  /** A new value asks for new pages. */
  version: number;
  delay?: number;
};

/** The invoice pages from the core, as pictures; the archived PDF once issued. */
export function PdfPages({ id, version, delay = PREVIEW_DELAY }: Props) {
  const { t } = useTranslation();
  const [pages, setPages] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: version signals new content
  useEffect(() => {
    let live = true;
    setBusy(true);
    const timer = setTimeout(() => {
      previewPng(id, WIDTH_PX)
        .then((found) => {
          if (!live) return;
          setPages(found);
          setError(null);
        })
        .catch((e) => live && setError(messageFor(toCmdError(e), t)))
        .finally(() => live && setBusy(false));
    }, delay);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [id, version, delay, t]);

  return (
    <div className="preview" aria-busy={busy}>
      <p className="preview-state" aria-live="polite">
        {busy ? (
          <>
            <LoaderCircle size={14} className="spin" aria-hidden="true" />
            {t("updating")}
          </>
        ) : pages.length > 1 ? (
          t("{n} pages", { n: String(pages.length) })
        ) : null}
      </p>
      {error && (
        <p className="preview-error" role="alert">
          <CircleAlert size={16} aria-hidden="true" />
          {error}
        </p>
      )}
      {pages.map((png, i) => (
        <img
          // biome-ignore lint/suspicious/noArrayIndexKey: pages have no other identity
          key={i}
          className={busy ? "page-image stale" : "page-image"}
          src={`data:image/png;base64,${png}`}
          alt={t("Page {n}", { n: String(i + 1) })}
        />
      ))}
    </div>
  );
}

/** What the core still misses before the draft can be issued, right beside the paper. */
function StillOpen({ id, version }: { id: string; version: number }) {
  const { t } = useTranslation();
  const [issues, setIssues] = useState<Issue[] | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: version signals a new save
  useEffect(() => {
    let live = true;
    const timer = setTimeout(() => {
      checkIssue(id)
        .then((found) => live && setIssues(found.filter((i) => i.severity === "error")))
        .catch(() => live && setIssues(null));
    }, PREVIEW_DELAY);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [id, version]);

  if (!issues) return null;
  if (issues.length === 0)
    return (
      <p className="still-open done">
        <CircleCheck size={16} aria-hidden="true" />
        {t("All required details are there.")}
      </p>
    );
  return (
    <div className="still-open">
      <p className="still-open-title">
        <CircleAlert size={16} aria-hidden="true" />
        {t("Still open:")}
      </p>
      <ul>
        {issues.map((issue, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: findings have no other identity
          <li key={i}>{issueText(issue.kind, t)}</li>
        ))}
      </ul>
    </div>
  );
}

/** The draft's pages beside the assistant, refreshed after every save. */
export function Preview() {
  const { t } = useTranslation();
  const id = useDraft((s) => s.id);
  const savedCount = useDraft((s) => s.savedCount);
  if (!id)
    return <p className="preview-empty">{t("The preview appears once the sender is chosen.")}</p>;
  return (
    <>
      <PdfPages id={id} version={savedCount} />
      <StillOpen id={id} version={savedCount} />
    </>
  );
}
