import { CircleAlert, CircleCheck, type LucideIcon, OctagonAlert, PencilLine } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { formatCents } from "../../format";
import type { Level } from "../../ipc/bindings/Level";
import type { SmallBusinessMeter as Meter } from "../../ipc/bindings/SmallBusinessMeter";
import { marks, rich } from "../../ui/rich";

/** The limits of § 19 UStG in cents, as the core uses them. */
const PREVIOUS_LIMIT = 2_500_000;
const CURRENT_LIMIT = 10_000_000;

const ICONS: Record<Level, LucideIcon> = {
  ok: CircleCheck,
  warning: CircleAlert,
  over: OctagonAlert,
};

type RowProps = { label: string; amount: number; limit: number; level: Level };

/** One amount against one limit: a bar with a mark at 80 % and the level in words. */
function MeterRow({ label, amount, limit, level }: RowProps) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const id = useId();
  const words: Record<Level, string> = {
    ok: t("below the limit"),
    warning: t("close to the limit"),
    over: t("above the limit"),
  };
  const Icon = ICONS[level];
  const money = {
    amount: formatCents(amount, "EUR", lang),
    limit: formatCents(limit, "EUR", lang),
  };
  const value = t("{amount} of {limit}", money);
  // The share only sets the bar's length; every amount shown comes from the core.
  const share = Math.min(100, Math.max(0, (amount / limit) * 100));
  return (
    <div className="meter-row">
      <div className="meter-head">
        <span id={id} className="meter-label">
          {label}
        </span>
        <span className="meter-value">
          {rich(t("{amount} of {limit}", marks("amount", "limit")), {
            amount: <span className="num">{money.amount}</span>,
            limit: <span className="num">{money.limit}</span>,
          })}
        </span>
      </div>
      {/* biome-ignore lint/a11y/useSemanticElements: a native meter cannot show the 80 % mark */}
      <div
        className="meter-track"
        role="meter"
        aria-labelledby={id}
        aria-valuemin={0}
        aria-valuemax={limit}
        aria-valuenow={Math.min(amount, limit)}
        aria-valuetext={`${value}, ${words[level]}`}
      >
        <span className={`meter-fill ${level}`} style={{ width: `${share}%` }} />
        <span className="meter-mark" />
      </div>
      <span className={`badge level ${level}`}>
        <Icon size={14} aria-hidden="true" />
        {words[level]}
      </span>
    </div>
  );
}

/** The one sentence about what the levels mean, most serious first. */
function useConsequence() {
  const { t } = useTranslation();
  return (m: Meter) => {
    if (m.previous_level === "over")
      return t("Above €25,000 last year: the small business rule does not apply this year.");
    if (m.current_level === "over")
      return t("Above €100,000 this year: the rule ends with the sale that goes over the limit.");
    if (m.current_level === "warning")
      return t("Above €100,000 the rule ends within the same year.");
    if (m.next_year_level === "over")
      return t("Above €25,000 this year: the rule does not apply next year.");
    if (m.next_year_level === "warning")
      return t("Close to €25,000 this year: above it, the rule does not apply next year.");
    return t("Below both limits.");
  };
}

type Props = { meter: Meter; named: boolean; onEdit: () => void };

/** How close a small business is to the limits of § 19 UStG. A hint, not tax advice. */
export function SmallBusinessMeter({ meter, named, onEdit }: Props) {
  const { t } = useTranslation();
  const titleId = useId();
  const consequence = useConsequence();
  const y = meter.year;
  return (
    <section className="overview-section meter" aria-labelledby={titleId}>
      <h2 id={titleId}>
        {named
          ? t("Small business · {name}", { name: meter.business_name })
          : t("Small business limits")}
      </h2>
      <MeterRow
        label={t("Last year ({year})", { year: String(y - 1) })}
        amount={meter.previous}
        limit={PREVIOUS_LIMIT}
        level={meter.previous_level}
      />
      <MeterRow
        label={t("{year} so far, counts for {next}", { year: String(y), next: String(y + 1) })}
        amount={meter.current}
        limit={PREVIOUS_LIMIT}
        level={meter.next_year_level}
      />
      <MeterRow
        label={t("{year} so far, upper limit", { year: String(y) })}
        amount={meter.current}
        limit={CURRENT_LIMIT}
        level={meter.current_level}
      />
      <p className="meter-consequence">{consequence(meter)}</p>
      <p className="field-hint">
        {t("Counted: payments received and your own entries. A hint, not tax advice.")}
      </p>
      <button type="button" className="btn ghost meter-edit" onClick={onEdit}>
        <PencilLine size={16} aria-hidden="true" />
        {t("Revenue outside PsyBil…")}
      </button>
    </section>
  );
}
