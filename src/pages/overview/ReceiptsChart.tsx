import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatCents, formatWholeCents, monthName } from "../../format";
import type { MonthlyReceipts } from "../../ipc/bindings/MonthlyReceipts";

const H = 150;

/** The smallest round value at or above `cents` (1, 2, 2.5 or 5 times a power of ten). */
function niceCeiling(cents: number): number {
  if (cents <= 0) return 0;
  const power = 10 ** Math.floor(Math.log10(cents));
  for (const f of [1, 2, 2.5, 5]) if (f * power >= cents) return f * power;
  return 10 * power;
}

/** A bar with rounded top corners standing on the baseline. */
function barPath(x: number, h: number, bar: number): string {
  const r = Math.min(4, bar / 2, h);
  const top = H - h;
  return `M${x},${H} V${top + r} Q${x},${top} ${x + r},${top} H${x + bar - r} Q${x + bar},${top} ${x + bar},${top + r} V${H} Z`;
}

type Props = { year: number; monthly: MonthlyReceipts; named: boolean };

/** Payments received per month of the year, one bar each; the values come from the core. */
export function ReceiptsChart({ year, monthly, named }: Props) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const titleId = useId();
  const [hover, setHover] = useState<number | null>(null);
  // Drawn at the plot's real width, so the rounded corners are not stretched.
  const plot = useRef<HTMLDivElement>(null);
  const [W, setW] = useState(560);
  useEffect(() => {
    const el = plot.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(([entry]) => {
      if (entry && entry.contentRect.width > 0) setW(entry.contentRect.width);
    });
    watch.observe(el);
    return () => watch.disconnect();
  }, []);
  const SLOT = W / 12;
  const BAR = Math.max(4, SLOT - Math.min(10, SLOT * 0.3));
  const title = named
    ? t("Payments received {year} · {currency}", { year: String(year), currency: monthly.currency })
    : t("Payments received {year}", { year: String(year) });
  const months = monthly.months;
  const top = niceCeiling(Math.max(0, ...months));
  const money = (cents: number) => formatCents(cents, monthly.currency, lang);

  return (
    <section className="overview-section chart" aria-labelledby={titleId}>
      <h2 id={titleId}>{title}</h2>
      {top === 0 ? (
        <p className="field-hint">
          {t("No payments received in {year} yet.", { year: String(year) })}
        </p>
      ) : (
        <div className="chart-frame">
          <div className="chart-scale" aria-hidden="true">
            <span className="num">{formatWholeCents(top, monthly.currency, lang)}</span>
            <span className="num">{formatWholeCents(top / 2, monthly.currency, lang)}</span>
          </div>
          <div className="chart-plot" ref={plot}>
            <svg viewBox={`0 0 ${W} ${H}`} aria-hidden="true">
              <line className="chart-grid" x1={0} x2={W} y1={0.5} y2={0.5} />
              <line className="chart-grid" x1={0} x2={W} y1={H / 2} y2={H / 2} />
              <line className="chart-base" x1={0} x2={W} y1={H - 0.5} y2={H - 0.5} />
              {months.map((cents, i) => {
                const h = cents > 0 ? Math.max(2, (cents / top) * H) : 0;
                const x = i * SLOT + (SLOT - BAR) / 2;
                return (
                  // biome-ignore lint/suspicious/noArrayIndexKey: the twelve months are fixed
                  // biome-ignore lint/a11y/noStaticElementInteractions: hover only adds a tooltip; the table below holds every value
                  <g key={i} onMouseEnter={() => setHover(i)} onMouseLeave={() => setHover(null)}>
                    <rect className="chart-hit" x={i * SLOT} y={0} width={SLOT} height={H} />
                    {h > 0 && (
                      <path
                        className={hover === i ? "chart-bar hover" : "chart-bar"}
                        d={barPath(x, h, BAR)}
                      />
                    )}
                  </g>
                );
              })}
            </svg>
            {hover !== null && (
              <div className="chart-tip" style={{ left: `${((hover + 0.5) / 12) * 100}%` }}>
                <span>{monthName(hover + 1, lang)}</span>
                <span className="num">{money(months[hover] ?? 0)}</span>
              </div>
            )}
            <div className="chart-months" aria-hidden="true">
              {months.map((_, i) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: the twelve months are fixed
                <span key={i}>{monthName(i + 1, lang, true)}</span>
              ))}
            </div>
          </div>
        </div>
      )}
      <table className="visually-hidden">
        <caption>{title}</caption>
        <thead>
          <tr>
            <th scope="col">{t("Month")}</th>
            <th scope="col">{t("Received")}</th>
          </tr>
        </thead>
        <tbody>
          {months.map((cents, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: the twelve months are fixed
            <tr key={i}>
              <th scope="row">{monthName(i + 1, lang)}</th>
              <td>{money(cents)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
