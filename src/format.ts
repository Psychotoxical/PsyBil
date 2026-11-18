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

const MONTH_NAMES: Record<Lang, string[]> = {
  de: [
    "Januar",
    "Februar",
    "März",
    "April",
    "Mai",
    "Juni",
    "Juli",
    "August",
    "September",
    "Oktober",
    "November",
    "Dezember",
  ],
  en: [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
  ],
};

/** The name of month 1–12, long ("März") or short ("Mär", "Mar"). */
export function monthName(month: number, code: string, short = false): string {
  const name = MONTH_NAMES[lang(code)][month - 1] ?? String(month);
  return short ? name.slice(0, 3) : name;
}

/** Cents from the core as the decimal string an input field takes: 123450 → "1234.50". */
export function centsToDecimal(cents: number): string {
  const abs = Math.abs(Math.trunc(cents));
  const sign = cents < 0 ? "-" : "";
  return `${sign}${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, "0")}`;
}

/** A moment "2026-11-18T09:05:00Z" in local time: "18.11.2026, 10:05" or "18 Nov 2026, 10:05". */
export function formatDateTime(iso: string, code: string): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, "0");
  const day = `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;
  return `${formatDate(day, code)}, ${pad(at.getHours())}:${pad(at.getMinutes())}`;
}

/** An amount in whole units for chart scales: 2500000 → "25.000 €" or "€25,000". */
export function formatWholeCents(cents: number, currency: string, code: string): string {
  const whole = Math.round(Math.abs(cents) / 100);
  const number = grouped(String(whole), lang(code) === "de" ? "." : ",");
  return withCurrency(cents < 0, number, currency, code);
}
