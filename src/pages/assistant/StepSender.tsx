import { useTranslation } from "react-i18next";
import type { Brand } from "../../ipc/bindings/Brand";
import type { Business } from "../../ipc/bindings/Business";
import { useDraft } from "../../store/draft";
import { Answer, newInput, Question } from "./shared";

type Props = { brands: Brand[]; businesses: Business[]; onDone: () => void };

/** Step 1: which brand issues the invoice. */
export function StepSender({ brands, businesses, onDone }: Props) {
  const { t, i18n } = useTranslation();
  const input = useDraft((s) => s.input);
  const change = useDraft((s) => s.change);

  /** A new sender brings its own terms and texts. With lines on the invoice the price mode
   *  stays, so the prices already entered keep their meaning. */
  const pick = (brand: Brand) => {
    const fresh = newInput(brand, i18n.language);
    if (!input) change(fresh);
    else if (input.brand_id !== brand.id)
      change({
        ...input,
        brand_id: brand.id,
        payment_days: fresh.payment_days,
        price_mode: input.lines.length > 0 ? input.price_mode : fresh.price_mode,
        intro_text: fresh.intro_text,
        outro_text: fresh.outro_text,
      });
    onDone();
  };

  return (
    <Question
      title={t("Who issues the invoice?")}
      sub={t("Each sender has its own logo, bank account and invoice numbers.")}
    >
      {brands.length === 0 ? (
        <p className="question-sub">{t("Add your business under Senders first.")}</p>
      ) : (
        <div className="answers">
          {brands.map((brand) => {
            const business = businesses.find((b) => b.id === brand.business_id);
            const sub = [
              business && business.name !== brand.name ? business.name : "",
              business
                ? business.small_business
                  ? t("Small business · § 19")
                  : t("Regular VAT")
                : "",
            ]
              .filter(Boolean)
              .join(" · ");
            return (
              <Answer
                key={brand.id}
                title={brand.name}
                sub={sub || undefined}
                icon={<span className="answer-swatch" style={{ background: brand.color }} />}
                pressed={input?.brand_id === brand.id}
                onClick={() => pick(brand)}
              />
            );
          })}
        </div>
      )}
    </Question>
  );
}
