import { useState } from "react";
import { useTranslation } from "react-i18next";
import { formatDate } from "../../format";
import type { ServiceDate } from "../../ipc/bindings/ServiceDate";
import { useDraft } from "../../store/draft";
import { Choice } from "../../ui/Choice";
import { TextField } from "../../ui/TextField";
import { addDays, Question } from "./shared";

const TERMS = [0, 7, 14, 30];

type Kind = ServiceDate["type"];

/** Step 4: invoice date, service date and payment term. */
export function StepDates() {
  const { t, i18n } = useTranslation();
  const input = useDraft((s) => s.input);
  const change = useDraft((s) => s.change);
  // The own term keeps its text while typing, so "75" does not stop at the 7-day button.
  const [own, setOwn] = useState<string | null>(() =>
    input && !TERMS.includes(input.payment_days) ? String(input.payment_days) : null,
  );
  if (!input) return null;

  const service = input.service_date;
  const setService = (next: ServiceDate) => change({ ...input, service_date: next });
  const setKind = (kind: Kind) => {
    const day = service.type === "day" ? service.date : input.invoice_date;
    if (kind === "same_as_invoice") setService({ type: "same_as_invoice" });
    else if (kind === "day") setService({ type: "day", date: day });
    else if (kind === "period")
      setService({ type: "period", from: day, to: service.type === "period" ? service.to : day });
    else setService({ type: "none" });
  };

  const kinds: { value: Kind; label: string }[] = [
    { value: "same_as_invoice", label: t("Same as the invoice date") },
    { value: "day", label: t("One day") },
    { value: "period", label: t("A period") },
  ];
  if (service.type === "none") kinds.push({ value: "none", label: t("None given") });

  const custom = own !== null;
  const term = custom ? -1 : input.payment_days;

  return (
    <Question
      title={t("When was it, and by when is it paid?")}
      sub={t(
        "The service date must be on the invoice. Usually it is the day of the show or the delivery.",
      )}
    >
      <div className="dates">
        <div className="field-grid">
          <TextField
            label={t("Invoice date")}
            type="date"
            value={input.invoice_date}
            onChange={(v) => v && change({ ...input, invoice_date: v })}
          />
        </div>

        <Choice label={t("Service date")} value={service.type} options={kinds} onChange={setKind} />
        {service.type === "day" && (
          <div className="field-grid">
            <TextField
              label={t("Day of the service")}
              type="date"
              value={service.date}
              onChange={(v) => v && setService({ type: "day", date: v })}
            />
          </div>
        )}
        {service.type === "period" && (
          <div className="field-grid">
            <TextField
              label={t("From")}
              type="date"
              value={service.from}
              onChange={(v) => v && setService({ ...service, from: v })}
            />
            <TextField
              label={t("To")}
              type="date"
              value={service.to}
              onChange={(v) => v && setService({ ...service, to: v })}
            />
          </div>
        )}

        <Choice
          label={t("Payment term")}
          value={term}
          options={[
            { value: 0, label: t("At once") },
            { value: 7, label: t("{n} days", { n: "7" }) },
            { value: 14, label: t("{n} days", { n: "14" }) },
            { value: 30, label: t("{n} days", { n: "30" }) },
            { value: -1, label: t("Own term") },
          ]}
          onChange={(v) => {
            if (v === -1) {
              setOwn(String(input.payment_days));
              return;
            }
            setOwn(null);
            change({ ...input, payment_days: v });
          }}
        />
        {own !== null && (
          <div className="field-grid">
            <TextField
              label={t("Days until payment")}
              value={own}
              unit={t("days")}
              inputMode="numeric"
              invalid={!/^\d{1,3}$/.test(own)}
              mono
              onChange={(v) => {
                setOwn(v);
                if (/^\d{1,3}$/.test(v)) change({ ...input, payment_days: Number(v) });
              }}
            />
          </div>
        )}
        <p className="due">
          {t("Due on {date}", {
            date: formatDate(addDays(input.invoice_date, input.payment_days), i18n.language),
          })}
        </p>
      </div>
    </Question>
  );
}
