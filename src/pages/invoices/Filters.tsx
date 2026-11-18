import { Search, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Business } from "../../ipc/bindings/Business";
import type { StatusKind } from "../../ipc/bindings/StatusKind";
import { EMPTY_FILTER, useInvoices } from "../../store/invoices";
import { SelectField } from "../../ui/SelectField";
import { useStatusLabels } from "../../ui/StatusBadge";

const ALL = "";
const STATUSES: StatusKind[] = ["draft", "open", "partly_paid", "paid", "overpaid", "cancelled"];

type Props = { businesses: Business[]; years: number[] };

/** Sender, status, year and a search; all of it goes to the core as `InvoiceFilter`. */
export function Filters({ businesses, years }: Props) {
  const { t } = useTranslation();
  const labels = useStatusLabels();
  const filter = useInvoices((s) => s.filter);
  const setFilter = useInvoices((s) => s.setFilter);
  const [text, setText] = useState(filter.text);

  // Typing waits a moment before it asks the core again.
  useEffect(() => {
    if (text === useInvoices.getState().filter.text) return;
    const timer = setTimeout(() => setFilter({ text }), 200);
    return () => clearTimeout(timer);
  }, [text, setFilter]);

  useEffect(() => {
    if (filter.text === "") setText("");
  }, [filter.text]);

  const active = JSON.stringify(filter) !== JSON.stringify(EMPTY_FILTER);

  return (
    <div className="filters">
      <label className="search">
        <Search size={16} aria-hidden="true" />
        <input
          type="search"
          autoComplete="off"
          spellCheck={false}
          value={text}
          placeholder={t("Number or customer")}
          aria-label={t("Search invoices")}
          onChange={(e) => setText(e.target.value)}
        />
      </label>
      {businesses.length > 1 && (
        <SelectField
          label={t("Sender")}
          value={filter.business_id ?? ALL}
          onChange={(v) => setFilter({ business_id: v || null })}
          options={[
            { value: ALL, label: t("All") },
            ...businesses.map((b) => ({ value: b.id, label: b.name })),
          ]}
        />
      )}
      <SelectField
        label={t("Status")}
        value={filter.status ?? ALL}
        onChange={(v) => setFilter({ status: (v || null) as StatusKind | null })}
        options={[
          { value: ALL, label: t("All") },
          ...STATUSES.map((s) => ({ value: s, label: labels[s] })),
        ]}
      />
      <SelectField
        label={t("Year")}
        value={filter.year === null ? ALL : String(filter.year)}
        onChange={(v) => setFilter({ year: v ? Number(v) : null })}
        options={[
          { value: ALL, label: t("All") },
          ...years.map((y) => ({ value: String(y), label: String(y) })),
        ]}
      />
      {active && (
        <button
          type="button"
          className="btn ghost"
          onClick={() => useInvoices.getState().setFilter(EMPTY_FILTER)}
        >
          <X size={16} aria-hidden="true" />
          {t("Reset filters")}
        </button>
      )}
    </div>
  );
}
