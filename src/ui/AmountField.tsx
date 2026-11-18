import { DecimalField, type DecimalProps } from "./DecimalField";

const SYMBOLS: Record<string, string> = { EUR: "€", USD: "$", GBP: "£" };

/** An amount of money as a decimal string, with the currency inside the box: "12,50 €". */
export function AmountField({ currency, ...props }: DecimalProps & { currency: string }) {
  return <DecimalField {...props} unit={SYMBOLS[currency] ?? currency} fraction={2} />;
}
