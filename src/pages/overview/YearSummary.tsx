import { FileDown, FileSpreadsheet } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatCents, formatDate, monthName } from "../../format";
import type { Business } from "../../ipc/bindings/Business";
import type { YearMonth } from "../../ipc/bindings/YearMonth";
import type { YearSummary as Summary } from "../../ipc/bindings/YearSummary";
import { messageFor, toCmdError } from "../../ipc/errors";
import { exportFileName, exportYearReport } from "../../ipc/exports";
import { listInvoices } from "../../ipc/invoices";
import { yearSummary } from "../../ipc/overview";
import { saveExport } from "../../shell/saveExport";
import { useUi } from "../../store/ui";
import { marks, rich } from "../../ui/rich";
import { SelectField } from "../../ui/SelectField";

type Props = {
  businesses: Business[];
  /** The sender the overview shows; with "all", the first one. */
  business: string | null;
  year?: number;
};

/** Per month what was invoiced and received in one year of one business, to save as PDF or CSV. */
export function YearSummary({ businesses, business, year: initialYear }: Props) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const toast = useUi((s) => s.toast);
  const titleId = useId();
  const thisYear = new Date().getFullYear();
  const [businessId, setBusinessId] = useState(business ?? businesses[0]?.id ?? "");
  const [year, setYear] = useState(initialYear ?? thisYear);
  const [years, setYears] = useState<number[]>([]);
  const [summary, setSummary] = useState<Summary | null>(null);

  useEffect(() => {
    setBusinessId(business ?? businesses[0]?.id ?? "");
  }, [business, businesses]);

  useEffect(() => {
    if (!businessId) return;
    let live = true;
    listInvoices({ business_id: businessId, status: null, year: null, text: "" })
      .then((rows) => {
        if (!live) return;
        const found = rows.map((r) => Number(r.invoice_date.slice(0, 4)));
        setYears([...new Set([thisYear, ...found])].sort((a, b) => b - a));
      })
      .catch(() => setYears([thisYear]));
    return () => {
      live = false;
    };
  }, [businessId, thisYear]);

  useEffect(() => {
    if (!businessId) return;
    let live = true;
    yearSummary(businessId, year)
      .then((found) => {
        if (live) setSummary(found);
      })
      .catch((error) => toast("error", messageFor(toCmdError(error), t)));
    return () => {
      live = false;
    };
  }, [businessId, year, toast, t]);

  if (!businessId) return null;

  const download = async (format: "pdf" | "csv") => {
    try {
      const name = await exportFileName(
        format === "pdf" ? "year_pdf" : "year_csv",
        businessId,
        year,
      );
      await saveExport(t, {
        name,
        type: format === "pdf" ? "PDF" : "CSV",
        extension: format,
        write: (path) => exportYearReport(businessId, year, path, format),
      });
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t));
    }
  };

  const money = (cents: number) => (
    <td className={cents === 0 ? "num end zero" : "num end"}>
      {formatCents(cents, summary?.currency ?? "EUR", lang)}
    </td>
  );
  const row = (m: YearMonth, label: string) => (
    <tr key={m.month}>
      <th scope="row">{label}</th>
      <td className={m.count === 0 ? "num end zero" : "num end"}>{m.count}</td>
      {money(m.net)}
      {money(m.tax)}
      {money(m.gross)}
      {money(m.received)}
    </tr>
  );
  const yearOptions = (years.includes(year) ? years : [...years, year]).sort((a, b) => b - a);

  return (
    <section className="overview-section year-summary" aria-labelledby={titleId}>
      <div className="year-head">
        <h2 id={titleId}>{t("Year summary")}</h2>
        <div className="year-pick">
          {businesses.length > 1 && (
            <SelectField
              label={t("Sender")}
              value={businessId}
              options={businesses.map((b) => ({ value: b.id, label: b.name }))}
              onChange={setBusinessId}
            />
          )}
          <SelectField
            label={t("Year")}
            value={String(year)}
            options={yearOptions.map((y) => ({ value: String(y), label: String(y) }))}
            onChange={(v) => setYear(Number(v))}
          />
        </div>
        <div className="year-actions">
          <button
            type="button"
            className="btn"
            disabled={!summary}
            onClick={() => void download("pdf")}
          >
            <FileDown size={16} aria-hidden="true" />
            {t("Save as PDF…")}
          </button>
          <button
            type="button"
            className="btn"
            disabled={!summary}
            onClick={() => void download("csv")}
          >
            <FileSpreadsheet size={16} aria-hidden="true" />
            {t("Save as CSV…")}
          </button>
        </div>
      </div>
      {summary && (
        <>
          <table className="year-table">
            <caption className="visually-hidden">
              {t("Year summary {year} · {name}", {
                year: String(summary.year),
                name: summary.business_name,
              })}
            </caption>
            <thead>
              <tr>
                <th scope="col">{t("Month")}</th>
                <th scope="col" className="end">
                  {t("Invoices")}
                </th>
                <th scope="col" className="end">
                  {t("Net")}
                </th>
                <th scope="col" className="end">
                  {t("Tax")}
                </th>
                <th scope="col" className="end">
                  {t("Gross")}
                </th>
                <th scope="col" className="end">
                  {t("Received")}
                </th>
              </tr>
            </thead>
            <tbody>{summary.months.map((m) => row(m, monthName(m.month, lang)))}</tbody>
            <tfoot>{row(summary.total, t("Total"))}</tfoot>
          </table>
          <p className="year-open">
            <span>
              {rich(t("Open on {date}", marks("date")), {
                date: <span className="num">{formatDate(summary.open_at, lang)}</span>,
              })}
            </span>
            <span className="num">{formatCents(summary.open_at_end, summary.currency, lang)}</span>
          </p>
        </>
      )}
    </section>
  );
}
