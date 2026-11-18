import { Check } from "lucide-react";
import { useTranslation } from "react-i18next";

export type Step = { title: string; sub?: string };

type Props = {
  steps: Step[];
  current: number;
  done: boolean[];
  /** Steps that cannot be opened yet. */
  disabled?: boolean[];
  onSelect: (index: number) => void;
};

/** Numbered steps across the top: Mauve for the current one, a tick when done. */
export function StepTiles({ steps, current, done, disabled, onSelect }: Props) {
  const { t } = useTranslation();
  return (
    <ol className="steps" aria-label={t("Steps")}>
      {steps.map((step, i) => {
        const state = i === current ? "current" : done[i] ? "done" : "todo";
        return (
          <li key={step.title}>
            <button
              type="button"
              className={`step ${state}`}
              aria-current={i === current ? "step" : undefined}
              disabled={disabled?.[i]}
              onClick={() => onSelect(i)}
            >
              <span className="step-mark num" aria-hidden="true">
                {state === "done" ? <Check size={14} strokeWidth={3} /> : i + 1}
              </span>
              <span className="step-text">
                <span className="step-title">{step.title}</span>
                {step.sub && <span className="step-sub">{step.sub}</span>}
              </span>
            </button>
          </li>
        );
      })}
    </ol>
  );
}
