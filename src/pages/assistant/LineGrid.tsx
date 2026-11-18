import { ArrowDown, ArrowUp, X } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatCents } from "../../format";
import type { Business } from "../../ipc/bindings/Business";
import type { LineDraft } from "../../ipc/bindings/LineDraft";
import type { TaxChoice } from "../../ipc/bindings/TaxChoice";
import { useDraft } from "../../store/draft";
import { decimals, formatDecimal, numberLang, parseDecimal } from "../../ui/number";
import { taxFromKey, taxKey } from "../catalog/ProductForm";

const SYMBOLS: Record<string, string> = { EUR: "€", USD: "$", GBP: "£" };
const currencySymbol = (currency: string) => SYMBOLS[currency] ?? currency;

/** A discount is entered as a positive price and stored as a negative one. */
const negate = (s: string) => (s.startsWith("-") ? s.slice(1) : s === "0" ? s : `-${s}`);

type CellProps = {
  label: string;
  value: string;
  onChange: (value: string) => void;
  fraction?: number;
  /** Shown inside the cell, such as the currency. */
  unit?: string;
  row: number;
  col: string;
};

/** A number in a grid cell, typed in the window's language; the core takes four decimals. */
function NumberCell({ label, value, onChange, fraction = 0, unit, row, col }: CellProps) {
  const { i18n } = useTranslation();
  const lang = numberLang(i18n.language);
  const [text, setText] = useState(() => formatDecimal(value, lang, fraction));
  const focused = useRef(false);
  useEffect(() => {
    if (!focused.current) setText(formatDecimal(value, lang, fraction));
  }, [value, lang, fraction]);
  const read = (raw: string) => {
    const parsed = parseDecimal(raw, lang);
    return parsed !== null && decimals(parsed) <= 4 ? parsed : null;
  };
  const invalid = read(text) === null;
  const input = (
    <input
      className="cell num end"
      aria-label={label}
      aria-invalid={invalid || undefined}
      inputMode="decimal"
      autoComplete="off"
      data-row={row}
      data-col={col}
      value={text}
      onFocus={() => {
        focused.current = true;
      }}
      onBlur={() => {
        focused.current = false;
        setText(formatDecimal(value, lang, fraction));
      }}
      onChange={(e) => {
        setText(e.target.value);
        const parsed = read(e.target.value);
        if (parsed !== null) onChange(parsed);
      }}
    />
  );
  if (!unit) return input;
  return (
    <span className={invalid ? "cell-box invalid" : "cell-box"}>
      {input}
      <span className="cell-unit" aria-hidden="true">
        {unit}
      </span>
    </span>
  );
}

type Props = { business: Business | undefined; currency: string };

/** The lines of the draft: Enter moves on, Alt+↑/↓ moves a line, Delete in an empty name removes it. */
export function LineGrid({ business, currency }: Props) {
  const { t, i18n } = useTranslation();
  const input = useDraft((s) => s.input);
  const keys = useDraft((s) => s.keys);
  const totals = useDraft((s) => s.totals);
  const { updateLine, moveLine, removeLine } = useDraft.getState();
  const grid = useRef<HTMLTableElement>(null);
  const focusNext = useRef<{ row: number; col: string } | null>(null);

  useLayoutEffect(() => {
    const target = focusNext.current;
    if (!target) return;
    focusNext.current = null;
    grid.current
      ?.querySelector<HTMLElement>(`[data-row="${target.row}"][data-col="${target.col}"]`)
      ?.focus();
  });

  if (!input || input.lines.length === 0) return null;
  const small = business?.small_business ?? false;

  const onKeyDown = (e: React.KeyboardEvent, row: number) => {
    const el = e.target as HTMLElement;
    const col = el.dataset.col;
    if (!col) return;
    if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      e.preventDefault();
      const by = e.key === "ArrowUp" ? -1 : 1;
      const to = row + by;
      if (to < 0 || to >= input.lines.length) return;
      focusNext.current = { row: to, col };
      moveLine(row, by);
      return;
    }
    if (e.key === "Enter" && el.tagName !== "SELECT") {
      e.preventDefault();
      const cells = [...(grid.current?.querySelectorAll<HTMLElement>("[data-col]") ?? [])];
      cells[cells.indexOf(el) + 1]?.focus();
      return;
    }
    if (e.key === "Delete" && col === "description" && (el as HTMLInputElement).value === "") {
      e.preventDefault();
      focusNext.current = { row: Math.max(0, row - 1), col: "description" };
      removeLine(row);
    }
  };

  const rates = business?.tax_rates ?? [];
  const taxOptions = (line: LineDraft) => {
    const options = rates.map((r) => ({
      value: `standard:${r}`,
      label: `${formatDecimal(r, numberLang(i18n.language))} %`,
    }));
    const own = taxKey(line.tax);
    if (line.kind === "shipping" || line.kind === "discount" || own === "follow_goods")
      options.push({ value: "follow_goods", label: t("like the goods") });
    if (!options.some((o) => o.value === own))
      options.push({
        value: own,
        label: own.startsWith("standard:")
          ? `${formatDecimal(own.slice(9), numberLang(i18n.language))} %`
          : own === "exempt"
            ? t("Tax exempt")
            : t("other"),
      });
    return options;
  };
  const reasonOf = (tax: TaxChoice) =>
    tax.mode === "fixed" && tax.category.type === "exempt" ? tax.category.reason : "";

  return (
    <table className="line-grid" ref={grid}>
      <thead>
        <tr>
          <th scope="col">{t("Designation")}</th>
          <th scope="col" className="end">
            {t("Quantity")}
          </th>
          <th scope="col">{t("Unit")}</th>
          <th scope="col" className="end">
            {t("Unit price")}
          </th>
          {!small && <th scope="col">{t("Tax")}</th>}
          <th scope="col" className="end">
            {t("Amount")}
          </th>
          <th scope="col">
            <span className="visually-hidden">{t("Actions")}</span>
          </th>
        </tr>
      </thead>
      <tbody>
        {input.lines.map((line, i) => {
          const n = String(i + 1);
          const discount = line.kind === "discount";
          const amount = totals?.line_amounts[i];
          return (
            <tr key={keys[i]} onKeyDown={(e) => onKeyDown(e, i)}>
              <td>
                <input
                  className="cell"
                  aria-label={t("Description, line {n}", { n })}
                  data-row={i}
                  data-col="description"
                  value={line.description}
                  placeholder={discount ? t("Discount") : t("What is it?")}
                  onChange={(e) => updateLine(i, { description: e.target.value })}
                />
              </td>
              <td className="qty">
                <NumberCell
                  label={t("Quantity, line {n}", { n })}
                  value={line.quantity}
                  row={i}
                  col="quantity"
                  onChange={(quantity) => updateLine(i, { quantity })}
                />
              </td>
              <td className="narrow">
                <input
                  className="cell"
                  aria-label={t("Unit, line {n}", { n })}
                  data-row={i}
                  data-col="unit"
                  value={line.unit}
                  onChange={(e) => updateLine(i, { unit: e.target.value })}
                />
              </td>
              <td className="price">
                <NumberCell
                  label={t("Unit price, line {n}", { n })}
                  value={discount ? negate(line.unit_price) : line.unit_price}
                  fraction={2}
                  unit={currencySymbol(currency)}
                  row={i}
                  col="price"
                  onChange={(v) => updateLine(i, { unit_price: discount ? negate(v) : v })}
                />
              </td>
              {!small && (
                <td className="narrow">
                  <select
                    className="cell"
                    aria-label={t("Tax, line {n}", { n })}
                    data-row={i}
                    data-col="tax"
                    value={taxKey(line.tax)}
                    onChange={(e) =>
                      updateLine(i, { tax: taxFromKey(e.target.value, reasonOf(line.tax)) })
                    }
                  >
                    {taxOptions(line).map((o) => (
                      <option key={o.value} value={o.value}>
                        {o.label}
                      </option>
                    ))}
                  </select>
                </td>
              )}
              <td className="num end amount">
                {amount === undefined ? "–" : formatCents(amount, currency, i18n.language)}
              </td>
              <td className="row-actions">
                <button
                  type="button"
                  className="icon-btn"
                  aria-label={t("Move line {n} up", { n })}
                  disabled={i === 0}
                  onClick={() => moveLine(i, -1)}
                >
                  <ArrowUp size={15} aria-hidden="true" />
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  aria-label={t("Move line {n} down", { n })}
                  disabled={i === input.lines.length - 1}
                  onClick={() => moveLine(i, 1)}
                >
                  <ArrowDown size={15} aria-hidden="true" />
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  aria-label={t("Remove line {n}", { n })}
                  onClick={() => removeLine(i)}
                >
                  <X size={15} aria-hidden="true" />
                </button>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
