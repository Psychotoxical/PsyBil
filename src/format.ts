/** Formats for amounts and dates, with the same rules as the PDF (`pdf/locale.rs`). */

export type Lang = "de" | "en";

const SYMBOLS: Record<string, string> = { EUR: "€", USD: "$", GBP: "£" };
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

function lang(code: string): Lang {
  return code.toLowerCase().startsWith("en") ? "en" : "de";
}

function grouped(digits: string, separator: string): string {
  return digits.replace(/\B(?=(\d{3})+(?!\d))/g, separator);
}

/** A sign, a number and a currency in the order of the language. */
export function withCurrency(negative: boolean, number: string, currency: string, code: string) {
  const sign = negative ? "-" : "";
  const symbol = SYMBOLS[currency];
  if (lang(code) === "de") return `${sign}${number} ${symbol ?? currency}`;
  return symbol ? `${sign}${symbol}${number}` : `${sign}${currency} ${number}`;
}

/** An amount in cents from the core, e.g. "1.234,56 €" or "€1,234.56". */
export function formatCents(cents: number, currency: string, code: string): string {
  const [thousands, decimal] = lang(code) === "de" ? [".", ","] : [",", "."];
  const abs = Math.abs(Math.trunc(cents));
  const whole = grouped(String(Math.floor(abs / 100)), thousands);
  const number = `${whole}${decimal}${String(abs % 100).padStart(2, "0")}`;
  return withCurrency(cents < 0, number, currency, code);
}

/** A date "YYYY-MM-DD" as "18.11.2026" or "18 Nov 2026". */
export function formatDate(iso: string, code: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  if (!y || !m || !d) return iso;
  if (lang(code) === "de")
    return `${String(d).padStart(2, "0")}.${String(m).padStart(2, "0")}.${y}`;
  return `${d} ${MONTHS[m - 1]} ${y}`;
}
