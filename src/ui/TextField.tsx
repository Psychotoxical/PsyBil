import { type HTMLAttributes, useId } from "react";
import { describedBy, Field } from "./Field";

type Props = {
  label: string;
  value: string;
  onChange: (value: string) => void;
  hint?: string;
  unit?: string;
  placeholder?: string;
  multiline?: boolean;
  type?: "text" | "email" | "url" | "tel" | "date";
  inputMode?: HTMLAttributes<HTMLInputElement>["inputMode"];
  maxLength?: number;
  invalid?: boolean;
  wide?: boolean;
  /** Numbers and codes are set in the mono font. */
  mono?: boolean;
};

/** A text input with its label; multiline for longer texts. */
export function TextField({
  label,
  value,
  onChange,
  hint,
  unit,
  placeholder,
  multiline,
  type = "text",
  inputMode,
  maxLength,
  invalid,
  wide,
  mono,
}: Props) {
  const id = useId();
  const common = {
    id,
    value,
    placeholder,
    maxLength,
    // The WebView's suggestion list would cover the form; PsyBil has its own lists.
    autoComplete: "off",
    className: mono ? "field-input num" : "field-input",
    "aria-invalid": invalid || undefined,
    "aria-describedby": describedBy(id, hint, unit),
  };
  return (
    <Field id={id} label={label} hint={hint} unit={unit} invalid={invalid} wide={wide || multiline}>
      {multiline ? (
        <textarea {...common} rows={3} onChange={(e) => onChange(e.target.value)} />
      ) : (
        <input
          {...common}
          type={type}
          inputMode={inputMode}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </Field>
  );
}
