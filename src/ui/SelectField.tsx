import { useId } from "react";
import { Field } from "./Field";

type Props<V extends string> = {
  label: string;
  value: V;
  options: { value: V; label: string }[];
  onChange: (value: V) => void;
  hint?: string;
  wide?: boolean;
};

/** A choice from a short list. */
export function SelectField<V extends string>({
  label,
  value,
  options,
  onChange,
  hint,
  wide,
}: Props<V>) {
  const id = useId();
  return (
    <Field id={id} label={label} hint={hint} wide={wide}>
      <select
        id={id}
        className="field-input"
        value={value}
        aria-describedby={hint ? `${id}-hint` : undefined}
        onChange={(e) => onChange(e.target.value as V)}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </Field>
  );
}
