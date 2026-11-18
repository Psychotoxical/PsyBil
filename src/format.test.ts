import { describe, expect, it } from "vitest";
import { formatCents, formatDate } from "./format";

describe("format", () => {
  it("formatCents_matches_pdf_rules", () => {
    expect(formatCents(123456, "EUR", "de")).toBe("1.234,56 €");
    expect(formatCents(123456, "EUR", "en")).toBe("€1,234.56");
    expect(formatCents(-2975, "EUR", "de")).toBe("-29,75 €");
    expect(formatCents(-2975, "EUR", "en")).toBe("-€29.75");
    expect(formatCents(123456, "USD", "en")).toBe("$1,234.56");
    expect(formatCents(123456, "CHF", "de")).toBe("1.234,56 CHF");
    expect(formatCents(123456, "CHF", "en")).toBe("CHF 1,234.56");
    expect(formatCents(5, "EUR", "de")).toBe("0,05 €");
    expect(formatCents(123456789, "EUR", "de")).toBe("1.234.567,89 €");
  });

  it("formatDate_matches_pdf_rules", () => {
    expect(formatDate("2026-11-18", "de")).toBe("18.11.2026");
    expect(formatDate("2026-11-18", "en")).toBe("18 Nov 2026");
    expect(formatDate("2026-03-01", "en")).toBe("1 Mar 2026");
  });
});
