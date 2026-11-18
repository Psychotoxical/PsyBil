import { DecimalField, type DecimalProps } from "./DecimalField";

/** A percentage such as a tax rate: "19 %", "16,5 %". */
export function PercentField(props: DecimalProps) {
  return <DecimalField {...props} unit="%" />;
}
