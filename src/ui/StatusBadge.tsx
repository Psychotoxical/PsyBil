import "./status.css";
import {
  AlarmClock,
  Ban,
  CircleCheck,
  CircleDashed,
  CirclePlus,
  Clock,
  type LucideIcon,
  PencilLine,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { InvoiceStatus } from "../ipc/bindings/InvoiceStatus";
import type { StatusKind } from "../ipc/bindings/StatusKind";

const ICONS: Record<StatusKind, LucideIcon> = {
  draft: PencilLine,
  open: Clock,
  partly_paid: CircleDashed,
  paid: CircleCheck,
  overpaid: CirclePlus,
  cancelled: Ban,
};

/** The name of a status in the window's language. */
export function useStatusLabels(): Record<StatusKind, string> {
  const { t } = useTranslation();
  return {
    draft: t("Draft"),
    open: t("Open"),
    partly_paid: t("Partly paid"),
    paid: t("Paid"),
    overpaid: t("Overpaid"),
    cancelled: t("Cancelled"),
  };
}

/** The state of an invoice in words, with an icon and a colour; overdue is an extra mark. */
export function StatusBadge({ status }: { status: InvoiceStatus }) {
  const { t } = useTranslation();
  const labels = useStatusLabels();
  const Icon = ICONS[status.kind];
  return (
    <span className="status">
      <span className={`badge ${status.kind}`}>
        <Icon size={14} aria-hidden="true" />
        {labels[status.kind]}
      </span>
      {status.overdue && (
        <span className="badge overdue">
          <AlarmClock size={14} aria-hidden="true" />
          {t("Overdue")}
        </span>
      )}
    </span>
  );
}
