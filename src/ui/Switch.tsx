import { useId } from "react";

type Props = { label: string; checked: boolean; onChange: (checked: boolean) => void };

/** An on/off setting; a checkbox underneath, so keyboard and screen readers know it. */
export function Switch({ label, checked, onChange }: Props) {
  const id = useId();
  return (
    <label className="switch" htmlFor={id}>
      <input
        id={id}
        type="checkbox"
        role="switch"
        checked={checked}
        aria-checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="switch-track" aria-hidden="true" />
      <span>{label}</span>
    </label>
  );
}
