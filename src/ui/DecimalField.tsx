import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { describedBy, Field } from "./Field";
import { decimals, formatDecimal, numberLang, parseDecimal } from "./number";

export type DecimalProps = {
  label: string;
  /** The decimal string the core reads ("16.5"); "" when empty. */
  value: string;
  /** The parsed value while typing; "" when the text is empty or not a number (`valid` false). */
  onChange: (value: string, valid: boolean) => void;
  hint?: string;
  wide?: boolean;
  /** More decimal places than this are not a valid entry (the core takes four). */
  maxFraction?: number;
};

type Props = DecimalProps & { unit: string; fraction?: number };

/** A number field in the window's language, with its unit inside the box. */
export function DecimalField({
  label,
  value,
  onChange,
  hint,
  wide,
  unit,
  fraction = 0,
  maxFraction = 4,
}: Props) {
  const id = useId();
  const { i18n } = useTranslation();
  const lang = numberLang(i18n.language);
  const [text, setText] = useState(() => formatDecimal(value, lang, fraction));
  const focused = useRef(false);

  // Values from outside win while the field is not being typed in.
  useEffect(() => {
    if (!focused.current) setText(formatDecimal(value, lang, fraction));
  }, [value, lang, fraction]);

  const read = (raw: string) => {
    const parsed = parseDecimal(raw, lang);
    return parsed !== null && decimals(parsed) <= maxFraction ? parsed : null;
  };
  const invalid = text.trim() !== "" && read(text) === null;
  return (
    <Field id={id} label={label} hint={hint} unit={unit} invalid={invalid} wide={wide}>
      <input
        id={id}
        className="field-input num"
        inputMode="decimal"
        autoComplete="off"
        value={text}
        aria-invalid={invalid || undefined}
        aria-describedby={describedBy(id, hint, unit)}
        onFocus={() => {
          focused.current = true;
        }}
        onBlur={() => {
          focused.current = false;
          const parsed = read(text);
          if (parsed !== null) setText(formatDecimal(parsed, lang, fraction));
        }}
        onChange={(e) => {
          setText(e.target.value);
          const parsed = read(e.target.value);
          onChange(parsed ?? "", parsed !== null || e.target.value.trim() === "");
        }}
      />
    </Field>
  );
}
