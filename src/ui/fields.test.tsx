import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../i18n";
import { AmountField } from "./AmountField";
import { parseDecimal } from "./number";
import { PercentField } from "./PercentField";

describe("fields", () => {
  afterEach(() => setLanguage("de"));

  it("percent_field_shows_unit_and_parses_de_and_en", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    setLanguage("de");
    const first = render(<PercentField label="Steuersatz" value="" onChange={onChange} />);
    expect(screen.getByText("%")).toBeInTheDocument();
    await user.type(screen.getByLabelText("Steuersatz"), "16,5 %");
    expect(onChange).toHaveBeenLastCalledWith("16.5", true);
    first.unmount();

    setLanguage("en");
    onChange.mockClear();
    render(<PercentField label="Rate" value="" onChange={onChange} />);
    await user.type(screen.getByLabelText("Rate"), "16.5%");
    expect(onChange).toHaveBeenLastCalledWith("16.5", true);
  });

  it("percent_field_reports_text_that_is_not_a_number", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<PercentField label="Steuersatz" value="" onChange={onChange} />);
    await user.type(screen.getByLabelText("Steuersatz"), "1x");
    expect(onChange).toHaveBeenLastCalledWith("", false);
    expect(screen.getByLabelText("Steuersatz")).toHaveAttribute("aria-invalid", "true");
  });

  it("percent_field_shows_value_in_the_window_language", () => {
    setLanguage("de");
    render(<PercentField label="Steuersatz" value="16.5" onChange={() => {}} />);
    expect(screen.getByLabelText("Steuersatz")).toHaveValue("16,5");
  });

  it("parse_decimal_english_takes_commas_only_as_groups", () => {
    expect(parseDecimal("12,50", "en")).toBeNull();
    expect(parseDecimal("1,2", "en")).toBeNull();
    expect(parseDecimal("1,234.5", "en")).toBe("1234.5");
    expect(parseDecimal("12,345,678", "en")).toBe("12345678");
  });

  it("amount_field_refuses_too_many_decimals", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <AmountField label="Betrag" value="" currency="EUR" maxFraction={2} onChange={onChange} />,
    );
    await user.type(screen.getByLabelText("Betrag"), "12,345");
    expect(onChange).toHaveBeenLastCalledWith("", false);
    expect(screen.getByLabelText("Betrag")).toHaveAttribute("aria-invalid", "true");
  });

  it("field_unit_is_described", () => {
    render(<AmountField label="Betrag" value="" currency="EUR" onChange={() => {}} />);
    expect(screen.getByLabelText("Betrag")).toHaveAccessibleDescription("€");
  });

  it("parse_decimal_reads_both_languages", () => {
    expect(parseDecimal("1.234,56", "de")).toBe("1234.56");
    expect(parseDecimal("1,234.56", "en")).toBe("1234.56");
    expect(parseDecimal("12.5", "de")).toBe("12.5");
    expect(parseDecimal("1.000", "de")).toBe("1000");
    expect(parseDecimal(" 7 ", "de")).toBe("7");
    expect(parseDecimal("-3,5", "de")).toBe("-3.5");
    expect(parseDecimal("abc", "de")).toBeNull();
    expect(parseDecimal("", "de")).toBeNull();
    expect(parseDecimal("1,2,3", "de")).toBeNull();
  });
});
