import { AlarmClock } from "lucide-react";
import { useTranslation } from "react-i18next";
import { formatCents } from "../../format";
import type { CurrencyTiles } from "../../ipc/bindings/CurrencyTiles";

/** "1 invoice" or "3 invoices". */
export function useInvoiceCount() {
  const { t } = useTranslation();
  return (n: number) => (n === 1 ? t("1 invoice") : t("{n} invoices", { n: String(n) }));
}

/** Open, overdue and received this month, one band per currency. */
export function Tiles({ tiles }: { tiles: CurrencyTiles[] }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const count = useInvoiceCount();
  return (
    <div className="tiles-stack">
      {tiles.map((tile) => (
        <dl key={tile.currency} className="tiles">
          <div className="tile">
            <dt>{t("Open")}</dt>
            <dd className="tile-value num">{formatCents(tile.open, tile.currency, lang)}</dd>
            <dd className="tile-sub">{count(tile.open_count)}</dd>
          </div>
          <div className={tile.overdue_count > 0 ? "tile overdue" : "tile"}>
            <dt>
              {tile.overdue_count > 0 && <AlarmClock size={14} aria-hidden="true" />}
              {t("Overdue")}
            </dt>
            <dd className="tile-value num">{formatCents(tile.overdue, tile.currency, lang)}</dd>
            <dd className="tile-sub">{count(tile.overdue_count)}</dd>
          </div>
          <div className="tile">
            <dt>{t("Received this month")}</dt>
            <dd className="tile-value num">
              {formatCents(tile.received_this_month, tile.currency, lang)}
            </dd>
          </div>
        </dl>
      ))}
    </div>
  );
}
