import { CircleAlert, CircleCheck, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Issue } from "../../ipc/bindings/Issue";
import { messageFor, toCmdError } from "../../ipc/errors";
import { checkIssue, issueInvoice, peekNumber } from "../../ipc/invoices";
import { type IssueTarget, issueTarget, issueText } from "../../ipc/issueTexts";
import { flushDraft, useDraft } from "../../store/draft";
import { useUi } from "../../store/ui";
import { Question } from "./shared";

/** A translated sentence with `{number}` set in the mono font. */
function WithNumber({ text, number }: { text: string; number: string }) {
  const [before, after] = text.split("\u0000");
  return (
    <>
      {before}
      <b className="num">{number}</b>
      {after}
    </>
  );
}

type Props = { onJump: (target: IssueTarget) => void };

/** Step 5: what the core still misses, the number to come, and "Issue". */
export function StepIssue({ onJump }: Props) {
  const { t } = useTranslation();
  const toast = useUi((s) => s.toast);
  const navigate = useUi((s) => s.navigate);
  const bump = useUi((s) => s.bump);
  const id = useDraft((s) => s.id);
  const savedCount = useDraft((s) => s.savedCount);
  // Issuing freezes what the core has; after a failed save that is not what is on screen.
  const unsaved = useDraft((s) => s.save === "error");
  const [issues, setIssues] = useState<Issue[] | null>(null);
  const [number, setNumber] = useState<string | null>(null);
  const [paid, setPaid] = useState(false);
  const [busy, setBusy] = useState(false);

  // biome-ignore lint/correctness/useExhaustiveDependencies: savedCount signals a new save
  useEffect(() => {
    if (!id) return;
    let live = true;
    (async () => {
      await flushDraft();
      const [found, next] = await Promise.all([checkIssue(id), peekNumber(id)]);
      if (!live) return;
      setIssues(found);
      setNumber(next);
    })().catch((e) => live && toast("error", messageFor(toCmdError(e), t)));
    return () => {
      live = false;
    };
  }, [id, savedCount, toast, t]);

  const errors = (issues ?? []).filter((i) => i.severity === "error");
  const warnings = (issues ?? []).filter((i) => i.severity === "warning");

  const issue = async () => {
    if (!id || errors.length > 0 || busy || unsaved) return;
    setBusy(true);
    try {
      await flushDraft();
      const result = await issueInvoice(id, paid);
      useDraft.getState().open(null, null);
      bump();
      if (result.negative_stock.length > 0)
        toast(
          "info",
          t("Stock is now below zero for: {names}", { names: result.negative_stock.join(", ") }),
        );
      navigate({ page: "issued", id, fresh: true });
    } catch (e) {
      toast("error", messageFor(toCmdError(e), t));
      setBusy(false);
    }
  };

  const label = unsaved
    ? t("Save first")
    : errors.length === 0
      ? t("Issue")
      : errors.length === 1
        ? t("1 detail missing")
        : t("{n} details missing", { n: String(errors.length) });

  const row = (item: Issue, i: number) => (
    <li key={i} className={`issue ${item.severity}`}>
      {item.severity === "error" ? (
        <CircleAlert size={18} aria-hidden="true" />
      ) : (
        <TriangleAlert size={18} aria-hidden="true" />
      )}
      <span className="issue-text">{issueText(item.kind, t)}</span>
      <button type="button" className="btn ghost" onClick={() => onJump(issueTarget(item.kind))}>
        {t("Go to field")}
      </button>
    </li>
  );

  return (
    <Question
      title={t("All there? Then issue it.")}
      sub={t("PsyBil checks the details the law requires before the number is given.")}
    >
      <div className="issue-step">
        {issues && errors.length === 0 && (
          <p className="issue-ok">
            <CircleCheck size={18} aria-hidden="true" />
            {t("All required details are there.")}
          </p>
        )}
        {(errors.length > 0 || warnings.length > 0) && (
          <ul className="issues">
            {errors.map(row)}
            {warnings.map((w, i) => row(w, errors.length + i))}
          </ul>
        )}

        <label className="check">
          <input type="checkbox" checked={paid} onChange={(e) => setPaid(e.target.checked)} />
          <span>{t("Payment already received (for example in cash at the merch stand)")}</span>
        </label>

        {number && (
          <p className="issue-number">
            <WithNumber
              number={number}
              text={t(
                "When issued, the invoice gets the number {number} and is frozen. After that, changes go only through Correct: PsyBil cancels it and creates a new one.",
                { number: "\u0000" },
              )}
            />
          </p>
        )}

        <div>
          <button
            type="button"
            className="btn primary big"
            disabled={!issues || errors.length > 0 || busy || unsaved}
            onClick={() => void issue()}
          >
            {label}
          </button>
        </div>
      </div>
    </Question>
  );
}
