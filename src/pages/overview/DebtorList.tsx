import { AlarmClock, Clock } from "lucide-react";
import { useTranslation } from "react-i18next";
import { formatCents, formatDate } from "../../format";
import type { Debtor } from "../../ipc/bindings/Debtor";
import { useUi } from "../../store/ui";
import { marks, rich } from "../../ui/rich";

/** How the due date stands, in words. */
function useDueText() {
  const { t } = useTranslation();
  return (days: number) => {
    if (days > 1) return t("{n} days overdue", { n: String(days) });
    if (days === 1) return t("1 day overdue");
    if (days === 0) return t("due today");
    if (days === -1) return t("due tomorrow");
    return t("due in {n} days", { n: String(-days) });
  };
}

/** Open invoices, the longest overdue first; a click opens the invoice. */
export function DebtorList({ debtors }: { debtors: Debtor[] }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const navigate = useUi((s) => s.navigate);
  const due = useDueText();
  return (
    <ul className="debtors">
      {debtors.map((d) => {
        const overdue = d.days_overdue > 0;
        const Icon = overdue ? AlarmClock : Clock;
        return (
          <li key={d.invoice_id}>
            <button
              type="button"
              className="debtor"
              onClick={() => navigate({ page: "issued", id: d.invoice_id })}
            >
              <span className="debtor-who">
                <span className="debtor-name">{d.customer_name || t("No customer")}</span>
                <span className="debtor-meta">
                  <span className="num">{d.number}</span>
                  <span aria-hidden="true">·</span>
                  <span>
                    {rich(t("due on {date}", marks("date")), {
                      date: <span className="num">{formatDate(d.due_date, lang)}</span>,
                    })}
                  </span>
                </span>
              </span>
              <span className={overdue ? "badge overdue" : "badge open"}>
                <Icon size={14} aria-hidden="true" />
                {due(d.days_overdue)}
              </span>
              <span className="debtor-amount num">{formatCents(d.open, d.currency, lang)}</span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}
