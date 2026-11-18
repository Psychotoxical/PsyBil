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
          <span className="field-unit" aria-hidden="true">
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
