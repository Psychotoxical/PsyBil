import "./fields.css";
import type { ReactNode } from "react";

type Props = {
  id: string;
  label: string;
  /** A short help text under the field. */
  hint?: string;
  /** The unit inside the field, "%", "€" or "Tage". */
  unit?: string;
  invalid?: boolean;
  wide?: boolean;
  children: ReactNode;
};

/** The ids that describe an input: its hint and its unit, so a screen reader says both. */
export function describedBy(id: string, hint?: string, unit?: string): string | undefined {
  const ids = [hint ? `${id}-hint` : "", unit ? `${id}-unit` : ""].filter(Boolean);
  return ids.length > 0 ? ids.join(" ") : undefined;
}

/** A label, the input with its unit inside the box, and an optional hint. */
export function Field({ id, label, hint, unit, invalid, wide, children }: Props) {
  return (
    <div className={wide ? "field wide" : "field"}>
      <label className="field-label" htmlFor={id}>
        {label}
      </label>
      <div className={invalid ? "field-box invalid" : "field-box"}>
        {children}
        {unit && (
          <span className="field-unit" id={`${id}-unit`} aria-hidden="true">
            {unit}
          </span>
        )}
      </div>
      {hint && (
        <span className="field-hint" id={`${id}-hint`}>
          {hint}
        </span>
      )}
    </div>
  );
}
