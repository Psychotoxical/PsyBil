import { Plus, X } from "lucide-react";
import { type FormEvent, type ReactNode, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Business } from "../../ipc/bindings/Business";
import type { NewBusiness } from "../../ipc/bindings/NewBusiness";
import { messageFor, toCmdError } from "../../ipc/errors";
import { createBusiness, updateBusiness } from "../../ipc/masterdata";
import { useUi } from "../../store/ui";
import { FormBar, saveOnCtrlS } from "../../ui/FormBar";
import { PercentField } from "../../ui/PercentField";
import { SelectField } from "../../ui/SelectField";
import { Switch } from "../../ui/Switch";
import { TextField } from "../../ui/TextField";

type Draft = Omit<NewBusiness, "tax_rates" | "small_business_since"> & { since: string };
type Rate = { key: number; value: string };

/** The same defaults as `NewBusiness::default` in the core. */
const NEW_BUSINESS: NewBusiness = {
  name: "",
  street: "",
  zip: "",
  city: "",
  country: "DE",
  tax_number: "",
  vat_id: "",
  small_business: false,
  small_business_since: null,
  currency: "EUR",
  number_template: "{PREFIX}-{YYYY}-{NNNN}",
  storno_prefix: "ST",
  tax_rates: ["19", "7"],
};

const CURRENCIES = ["EUR", "CHF", "USD", "GBP"];

function toDraft(b: NewBusiness): Draft {
  return {
    name: b.name,
    street: b.street,
    zip: b.zip,
    city: b.city,
    country: b.country,
    tax_number: b.tax_number,
    vat_id: b.vat_id,
    small_business: b.small_business,
    since: b.small_business_since ?? "",
    currency: b.currency,
    number_template: b.number_template,
    storno_prefix: b.storno_prefix,
  };
}

let nextKey = 1;
const toRates = (rates: string[]): Rate[] => rates.map((value) => ({ key: nextKey++, value }));

type Props = {
  business: Business | null;
  onDone: (id: string) => void;
  onCancel: () => void;
  onDirty?: (dirty: boolean) => void;
  extra?: ReactNode;
};

/** The business that owes the taxes: address, tax details, numbering. */
export function BusinessForm({ business, onDone, onCancel, onDirty, extra }: Props) {
  const { t } = useTranslation();
  const toast = useUi((s) => s.toast);
  const formId = useId();
  const source = business ?? NEW_BUSINESS;
  const initial = useRef(toDraft(source));
  const [draft, setDraft] = useState(initial.current);
  const [rates, setRates] = useState(() => toRates(source.tax_rates));
  const [busy, setBusy] = useState(false);

  const values = rates.map((r) => r.value);
  const dirty =
    JSON.stringify(draft) !== JSON.stringify(initial.current) ||
    JSON.stringify(values) !== JSON.stringify(source.tax_rates);
  useEffect(() => onDirty?.(dirty), [dirty, onDirty]);

  const template = draft.number_template;
  const missing = [
    !draft.name.trim() && t("Name is missing"),
    !(template.includes("{N}") || template.includes("{NNNN}")) &&
      t("The number template needs {counter}", { counter: "{NNNN}" }),
    !draft.small_business && rates.length === 0 && t("At least one tax rate is needed"),
    ...rates.map((r, i) => {
      const n = Number(r.value);
      return (
        (r.value === "" || n < 0 || n > 100) && t("Tax rate {n} is not valid", { n: String(i + 1) })
      );
    }),
  ].filter((m): m is string => Boolean(m));

  const set =
    <K extends keyof Draft>(key: K) =>
    (value: Draft[K]) =>
      setDraft((d) => ({ ...d, [key]: value }));

  const discard = () => {
    setDraft(initial.current);
    setRates(toRates(source.tax_rates));
    onCancel();
  };

  const save = async (e?: FormEvent) => {
    e?.preventDefault();
    if (missing.length > 0 || busy) return;
    setBusy(true);
    const { since, ...fields } = draft;
    const input: NewBusiness = {
      ...fields,
      country: fields.country.trim().toUpperCase(),
      small_business_since: since || null,
      tax_rates: values,
    };
    try {
      if (business) {
        await updateBusiness({ ...business, ...input });
        onDone(business.id);
      } else {
        const [id] = await createBusiness(input);
        onDone(id);
      }
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t, "business"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form id={formId} className="edit-form" onSubmit={save} onKeyDown={saveOnCtrlS}>
      <h2 className="form-title">
        {business ? business.name || t("Business") : t("New business")}
      </h2>

      <section className="form-section">
        <h3>{t("Address")}</h3>
        <div className="field-grid">
          <TextField label={t("Name")} value={draft.name} onChange={set("name")} wide />
          <TextField label={t("Street")} value={draft.street} onChange={set("street")} wide />
          <TextField label={t("Postcode")} value={draft.zip} onChange={set("zip")} mono />
          <TextField label={t("City")} value={draft.city} onChange={set("city")} />
          <TextField
            label={t("Country")}
            value={draft.country}
            onChange={set("country")}
            hint={t("Two letters, as in {example}.", { example: "DE" })}
            maxLength={2}
            mono
          />
        </div>
      </section>

      <section className="form-section">
        <h3>{t("Taxes")}</h3>
        <div className="field-grid">
          <TextField
            label={t("Tax number")}
            value={draft.tax_number}
            onChange={set("tax_number")}
            mono
          />
          <TextField label={t("VAT ID")} value={draft.vat_id} onChange={set("vat_id")} mono />
          <div className="field wide">
            <Switch
              label={t("Small business (§ 19 UStG)")}
              checked={draft.small_business}
              onChange={set("small_business")}
            />
            <span className="field-hint">
              {t("Invoices show no tax and carry the note on § 19 UStG.")}
            </span>
          </div>
          {draft.small_business && (
            <TextField
              label={t("Small business since")}
              type="date"
              value={draft.since}
              onChange={set("since")}
            />
          )}
          <div className="field wide">
            <span className="field-label">{t("Tax rates")}</span>
            <div className="rate-list">
              {rates.map((r, i) => (
                <div className="rate" key={r.key}>
                  <PercentField
                    label={t("Tax rate {n}", { n: String(i + 1) })}
                    value={r.value}
                    onChange={(value) =>
                      setRates((all) => all.map((x) => (x.key === r.key ? { ...x, value } : x)))
                    }
                  />
                  <button
                    type="button"
                    className="icon-btn"
                    aria-label={t("Remove tax rate {n}", { n: String(i + 1) })}
                    onClick={() => setRates((all) => all.filter((x) => x.key !== r.key))}
                  >
                    <X size={16} aria-hidden="true" />
                  </button>
                </div>
              ))}
              <button
                type="button"
                className="btn ghost"
                onClick={() => setRates((all) => [...all, { key: nextKey++, value: "" }])}
              >
                <Plus size={16} aria-hidden="true" />
                {t("Add tax rate")}
              </button>
            </div>
          </div>
        </div>
      </section>

      <section className="form-section">
        <h3>{t("Numbers")}</h3>
        <div className="field-grid">
          <TextField
            label={t("Number template")}
            value={draft.number_template}
            onChange={set("number_template")}
            hint={t("{prefix} prefix of the brand, {year} year, {counter} running number", {
              prefix: "{PREFIX}",
              year: "{YYYY}",
              counter: "{NNNN}",
            })}
            invalid={!(template.includes("{N}") || template.includes("{NNNN}"))}
            wide
            mono
          />
          <TextField
            label={t("Cancellation prefix")}
            value={draft.storno_prefix}
            onChange={set("storno_prefix")}
            mono
          />
          <SelectField
            label={t("Currency")}
            value={draft.currency}
            onChange={set("currency")}
            options={[...new Set([...CURRENCIES, draft.currency])].map((c) => ({
              value: c,
              label: c,
            }))}
          />
        </div>
      </section>

      <FormBar
        formId={formId}
        missing={missing}
        busy={busy}
        dirty={dirty}
        onDiscard={discard}
        extra={extra}
      />
    </form>
  );
}
