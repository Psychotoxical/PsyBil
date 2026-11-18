import type { Lang } from "../format";

/** The language of the window as a number format. */
export function numberLang(code: string): Lang {
  return code.toLowerCase().startsWith("en") ? "en" : "de";
}

/**
 * A number as typed, "16,5 %" or "1.234,56" (German) or "1,234.56" (English), as the decimal
 * string the core reads ("16.5", "1234.56"). Units and spaces are ignored; null when it is not
 * a number.
 */
export function parseDecimal(text: string, lang: Lang): string | null {
  let s = text.replace(/[\s %€$£]/g, "");
  if (s === "") return null;
  if (lang === "de") {
    const commas = s.split(",").length - 1;
    if (commas > 1) return null;
    if (commas === 1) s = s.replace(/\./g, "").replace(",", ".");
    else if (/^-?\d{1,3}(\.\d{3})+$/.test(s)) s = s.replace(/\./g, "");
  } else if (s.includes(",")) {
    // In English a comma only groups thousands; "12,50" is not a number there.
    if (!/^-?\d{1,3}(,\d{3})+(\.\d*)?$/.test(s)) return null;
    s = s.replace(/,/g, "");
  }
  if (!/^-?(\d+\.?\d*|\.\d+)$/.test(s)) return null;
  if (s.endsWith(".")) s = s.slice(0, -1);
  if (s.startsWith(".")) s = `0${s}`;
  if (s.startsWith("-.")) s = `-0${s.slice(1)}`;
  return s;
}

/** Decimal places of a decimal string: "12.345" → 3. */
export function decimals(value: string): number {
  const dot = value.indexOf(".");
  return dot < 0 ? 0 : value.length - dot - 1;
}

/** A decimal string from the core for an input field: "1234.5" → "1.234,50" (German). */
export function formatDecimal(value: string, lang: Lang, fraction = 0): string {
  const m = /^(-?)(\d*)(?:\.(\d*))?$/.exec(value.trim());
  if (!m || value.trim() === "") return value;
  const [, sign, whole = "", frac = ""] = m;
  const [thousands, decimal] = lang === "de" ? [".", ","] : [",", "."];
  const grouped = (whole || "0").replace(/\B(?=(\d{3})+(?!\d))/g, thousands);
  const digits = frac.padEnd(fraction, "0");
  return digits ? `${sign}${grouped}${decimal}${digits}` : `${sign}${grouped}`;
}
