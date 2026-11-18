import "./fields.css";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useReadOnly } from "../store/workspace";

type Props = {
  formId: string;
  /** What is still missing; while there is something, saving is not possible. */
  missing: string[];
  busy: boolean;
  dirty: boolean;
  onDiscard: () => void;
  /** Actions of the saved object, such as archive and delete. */
  extra?: ReactNode;
};

/** The bar under a form: further actions, what is missing, discard and save. */
export function FormBar({ formId, missing, busy, dirty, onDiscard, extra }: Props) {
  const { t } = useTranslation();
  const readOnly = useReadOnly();
  // While another computer writes, the reason replaces the list of what is missing.
  const reasons = readOnly ? [readOnly] : missing;
  const blocked = reasons.length > 0;
  return (
    <div className="form-bar">
      <div className="form-bar-extra">{extra}</div>
      {blocked && (
        <p className="form-missing" id={`${formId}-missing`} aria-live="polite">
          {reasons.map((m) => (
            <span key={m}>{m}</span>
          ))}
        </p>
      )}
      <button type="button" className="btn ghost" disabled={!dirty || busy} onClick={onDiscard}>
        {t("Discard")}
      </button>
      <button
        type="submit"
        form={formId}
        className="btn primary"
        disabled={blocked || busy}
        aria-describedby={blocked ? `${formId}-missing` : undefined}
        aria-keyshortcuts="Control+S"
      >
        {t("Save")}
      </button>
    </div>
  );
}

/** Ctrl+S saves the form at once. */
export function saveOnCtrlS(e: React.KeyboardEvent<HTMLFormElement>) {
  if (e.ctrlKey && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "s") {
    e.preventDefault();
    e.currentTarget.requestSubmit();
  }
}
