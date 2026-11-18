import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Field } from "./Field";

export const HEX_COLOR = /^#[0-9a-fA-F]{6}$/;

type Props = { label: string; value: string; onChange: (value: string) => void };

/** A colour as a swatch to pick from and as "#rrggbb" to type. */
export function ColorField({ label, value, onChange }: Props) {
  const { t } = useTranslation();
  const id = useId();
  const valid = HEX_COLOR.test(value);
  return (
    <Field id={id} label={label} invalid={!valid}>
      <input
        type="color"
        className="field-swatch"
        aria-label={t("Pick colour")}
        value={valid ? value.toLowerCase() : "#000000"}
        onChange={(e) => onChange(e.target.value)}
      />
      <input
        id={id}
        className="field-input num"
        value={value}
        maxLength={7}
        spellCheck={false}
        aria-invalid={!valid || undefined}
        onChange={(e) => onChange(e.target.value.trim())}
      />
    </Field>
  );
}
