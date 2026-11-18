import "./invoices.css";
import { ArrowDown, ArrowUp, ArrowUpDown, Ban, FileSpreadsheet, Plus } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatCents, formatDate } from "../../format";
import type { Business } from "../../ipc/bindings/Business";
import type { InvoiceRow } from "../../ipc/bindings/InvoiceRow";
import { messageFor, toCmdError } from "../../ipc/errors";
import { exportFileName, exportInvoicesCsv } from "../../ipc/exports";
import { listInvoices } from "../../ipc/invoices";
import { listBusinesses } from "../../ipc/masterdata";
import { PageHeader } from "../../shell/PageHeader";
import { saveExport } from "../../shell/saveExport";
import { EMPTY_FILTER, type SortKey, sortRows, useInvoices } from "../../store/invoices";
import { useUi } from "../../store/ui";
import { StatusBadge } from "../../ui/StatusBadge";
import { Filters } from "./Filters";

const isEmpty = (f: object) => JSON.stringify(f) === JSON.stringify(EMPTY_FILTER);
const yearsOf = (rows: InvoiceRow[]) =>
  [...new Set(rows.map((r) => Number(r.invoice_date.slice(0, 4))))].sort((a, b) => b - a);

/** All invoices and drafts, filtered by the core, in the order the person picks. */
export function InvoiceList() {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const toast = useUi((s) => s.toast);
  const navigate = useUi((s) => s.navigate);
  const filter = useInvoices((s) => s.filter);
  const sort = useInvoices((s) => s.sort);
  const sortBy = useInvoices((s) => s.sortBy);
  const [rows, setRows] = useState<InvoiceRow[] | null>(null);
  const [businesses, setBusinesses] = useState<Business[]>([]);
  const [years, setYears] = useState<number[] | null>(null);

  useEffect(() => {
    listBusinesses(false)
      .then(setBusinesses)
      .catch(() => {});
    // With a filter kept from earlier, the years come from an unfiltered list.
    if (!isEmpty(useInvoices.getState().filter))
      listInvoices(EMPTY_FILTER)
        .then((all) => setYears(yearsOf(all)))
        .catch(() => {});
  }, []);

  useEffect(() => {
    let live = true;
    listInvoices(filter)
      .then((found) => {
        if (!live) return;
        setRows(found);
        if (isEmpty(filter)) setYears(yearsOf(found));
      })
      .catch((error) => toast("error", messageFor(toCmdError(error), t)));
    return () => {
      live = false;
    };
  }, [filter, toast, t]);

  const senderName = (row: InvoiceRow) =>
    businesses.find((b) => b.id === row.business_id)?.name ?? "";
  const exportCsv = async () => {
    try {
      const name = await exportFileName("invoices_csv");
      // The file holds what the list shows: the filter as it is set now.
      const current = useInvoices.getState().filter;
      await saveExport(t, {
        name,
        type: "CSV",
        extension: "csv",
        write: (path) => exportInvoicesCsv(current, path),
      });
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t));
    }
  };

  const open = (row: InvoiceRow) =>
    navigate(
      row.state === "draft" ? { page: "assistant", id: row.id } : { page: "issued", id: row.id },
    );

  const columns: { key: SortKey; label: string; end?: boolean }[] = [
    { key: "number", label: t("Number") },
    { key: "date", label: t("Date") },
    { key: "customer", label: t("Customer") },
    { key: "amount", label: t("Amount"), end: true },
    { key: "status", label: t("Status") },
  ];

  const yearOptions = [...new Set([...(years ?? []), ...(filter.year ? [filter.year] : [])])].sort(
    (a, b) => b - a,
  );
  const sorted = rows ? sortRows(rows, sort) : [];
  const none = rows !== null && rows.length === 0 && isEmpty(filter);

  return (
    <>
      <PageHeader
        title={t("Invoices")}
        actions={
          !none && (
            <button type="button" className="btn" onClick={() => void exportCsv()}>
              <FileSpreadsheet size={16} aria-hidden="true" />
              {t("Export CSV…")}
            </button>
          )
        }
      />
      {none ? (
        <div className="empty">
          <h2>{t("No invoices yet")}</h2>
          <p>
            {t(
              "The assistant asks for the customer and the items, and shows the invoice as it grows.",
            )}
          </p>
          <button
            type="button"
            className="btn primary"
            onClick={() => navigate({ page: "assistant" })}
          >
            <Plus size={16} aria-hidden="true" />
            {t("New invoice")}
          </button>
        </div>
      ) : (
        <>
          <Filters businesses={businesses} years={yearOptions} />
          {rows && rows.length === 0 ? (
            <div className="empty">
              <h2>{t("No invoice matches these filters")}</h2>
              <button
                type="button"
                className="btn"
                onClick={() => useInvoices.getState().setFilter(EMPTY_FILTER)}
              >
                {t("Reset filters")}
              </button>
            </div>
          ) : (
            <table className="invoice-table">
              <thead>
                <tr>
                  {columns.map((c) => {
                    const current = sort.key === c.key;
                    const Icon = !current ? ArrowUpDown : sort.dir === "asc" ? ArrowUp : ArrowDown;
                    return (
                      <th
                        key={c.key}
                        scope="col"
                        className={c.end ? "end" : undefined}
                        aria-sort={
                          current ? (sort.dir === "asc" ? "ascending" : "descending") : undefined
                        }
                      >
                        <button
                          type="button"
                          className={current ? "sort current" : "sort"}
                          onClick={() => sortBy(c.key)}
                        >
                          {c.label}
                          <Icon size={14} aria-hidden="true" />
                        </button>
                      </th>
                    );
                  })}
                </tr>
              </thead>
              <tbody>
                {sorted.map((r) => (
                  <tr
                    key={r.id}
                    data-id={r.id}
                    tabIndex={0}
                    onClick={() => open(r)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") open(r);
                    }}
                  >
                    <td>
                      <span className="number-cell">
                        {r.number ? (
                          <span className="num">{r.number}</span>
                        ) : (
                          <span className="muted">{t("not yet numbered")}</span>
                        )}
                        {/* Each sender counts its own numbers; with several, say whose it is. */}
                        {businesses.length > 1 && (
                          <span className="row-sender">{senderName(r)}</span>
                        )}
                      </span>
                    </td>
                    <td className="num">{formatDate(r.invoice_date, lang)}</td>
                    <td className="customer">
                      {r.customer_name ?? <span className="muted">{t("No customer yet")}</span>}
                    </td>
                    <td className="num end">{formatCents(r.gross, r.currency, lang)}</td>
                    <td>
                      {r.kind === "storno" ? (
                        <span className="badge storno">
                          <Ban size={14} aria-hidden="true" />
                          {t("Cancellation")}
                        </span>
                      ) : (
                        <StatusBadge status={r.status} />
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </>
      )}
    </>
  );
}
