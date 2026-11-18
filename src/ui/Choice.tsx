import "./fields.css";

type Props<V extends string | number> = {
  label: string;
  value: V;
  options: { value: V; label: string }[];
  onChange: (value: V) => void;
};

/** A row of buttons of which one is chosen. */
export function Choice<V extends string | number>({ label, value, options, onChange }: Props<V>) {
  return (
    <fieldset className="choice">
      <legend className="field-label">{label}</legend>
      <div className="choice-row">
        {options.map((o) => (
          <button
            key={o.value}
            type="button"
            className="choice-btn"
            aria-pressed={o.value === value}
            onClick={() => onChange(o.value)}
          >
            {o.label}
          </button>
        ))}
      </div>
    </fieldset>
  );
}
