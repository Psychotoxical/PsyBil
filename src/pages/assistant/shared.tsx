import type { ReactNode } from "react";
import type { Brand } from "../../ipc/bindings/Brand";
import type { Business } from "../../ipc/bindings/Business";
import type { DraftInput } from "../../ipc/bindings/DraftInput";
import type { TaxChoice } from "../../ipc/bindings/TaxChoice";

/** Today as "YYYY-MM-DD" in local time. */
export function today(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** A calendar date plus days, as the core counts the due date. */
export function addDays(iso: string, days: number): string {
  const [y, m, d] = iso.split("-").map(Number);
  const date = new Date(Date.UTC(y ?? 1970, (m ?? 1) - 1, d ?? 1) + days * 86_400_000);
  return date.toISOString().slice(0, 10);
}

/** A new draft for a brand, with the brand's own terms and texts. */
export function newInput(brand: Brand, language: string): DraftInput {
  return {
    brand_id: brand.id,
    customer_id: null,
    invoice_date: today(),
    service_date: { type: "same_as_invoice" },
    payment_days: brand.payment_days,
    price_mode: brand.price_mode,
    language,
    intro_text: brand.intro_text,
    outro_text: brand.outro_text,
    notes: "",
    lines: [],
  };
}

/** The tax of a new free line: § 19 for small businesses, else the first rate. */
export function defaultTax(business: Business | undefined): TaxChoice {
  if (business?.small_business) return { mode: "fixed", category: { type: "small_business" } };
  return {
    mode: "fixed",
    category: { type: "standard", rate: business?.tax_rates[0] ?? "19" },
  };
}

type AnswerProps = {
  title: string;
  sub?: ReactNode;
  icon?: ReactNode;
  pressed?: boolean;
  onClick: () => void;
};

/** A large answer tile; choosing it answers the question. */
export function Answer({ title, sub, icon, pressed, onClick }: AnswerProps) {
  return (
    <button type="button" className="answer" aria-pressed={pressed} onClick={onClick}>
      {icon && <span className="answer-icon">{icon}</span>}
      <span className="answer-title">{title}</span>
      {sub && <span className="answer-sub">{sub}</span>}
    </button>
  );
}

type QuestionProps = { title: string; sub: string; children: ReactNode };

/** One question of the assistant with its answers. */
export function Question({ title, sub, children }: QuestionProps) {
  return (
    <section className="question-block" aria-labelledby="question">
      <h2 id="question" className="question" tabIndex={-1}>
        {title}
      </h2>
      <p className="question-sub">{sub}</p>
      {children}
    </section>
  );
}
