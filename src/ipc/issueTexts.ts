import type { IssueKind } from "./bindings/IssueKind";

type T = (key: string, values?: Record<string, string>) => string;

/** Where "Go to field" leads: a step of the assistant (and a line cell) or a page. */
export type IssueTarget =
  | { step: number; row?: number; col?: "description" | "tax" }
  | { page: "senders" };

/** A concrete sentence for a finding of `check_issue`, in the window's language. */
export function issueText(kind: IssueKind, t: T): string {
  switch (kind.type) {
    case "business_name":
      return t("The sender has no name.");
    case "business_address":
      return t("The sender's address is incomplete.");
    case "business_tax_id":
      return t("The sender needs a tax number or a VAT ID.");
    case "business_vat_id":
      return t("The sender needs a VAT ID for this kind of tax.");
    case "no_lines":
      return t("The invoice has no lines yet.");
    case "line_description":
      return t("Line {n} has no designation.", { n: String(kind.position) });
    case "customer":
      return t("No customer is chosen yet.");
    case "customer_address":
      return t("The customer's address is incomplete.");
    case "customer_kind_unknown":
      return t("It is not known whether the customer is a company or a private person.");
    case "customer_vat_id":
      return t("The customer needs a VAT ID for this kind of tax.");
    case "service_date":
      return t("The service date is missing.");
    case "service_period_order":
      return t("The service period ends before it starts.");
    case "exempt_reason":
      return t("Line {n} is tax exempt but gives no reason.", { n: String(kind.position) });
    case "tax_rate":
      return t("Line {n} has a tax rate that is not valid.", { n: String(kind.position) });
    case "small_business_line":
      return t("Line {n} shows tax, but a small business charges none.", {
        n: String(kind.position),
      });
    case "negative_total":
      return t("The total is below zero.");
  }
}

/** The step and field where a finding can be fixed. */
export function issueTarget(kind: IssueKind): IssueTarget {
  switch (kind.type) {
    case "business_name":
    case "business_address":
    case "business_tax_id":
    case "business_vat_id":
      return { page: "senders" };
    case "customer":
    case "customer_address":
    case "customer_kind_unknown":
    case "customer_vat_id":
      return { step: 1 };
    case "no_lines":
    case "negative_total":
      return { step: 2 };
    case "line_description":
      return { step: 2, row: kind.position - 1, col: "description" };
    case "exempt_reason":
    case "tax_rate":
    case "small_business_line":
      return { step: 2, row: kind.position - 1, col: "tax" };
    case "service_date":
    case "service_period_order":
      return { step: 3 };
  }
}
