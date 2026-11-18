import { Clock, Mic, Package, PenLine, Percent, Search, Truck } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { withCurrency } from "../../format";
import type { Business } from "../../ipc/bindings/Business";
import type { Product } from "../../ipc/bindings/Product";
import { messageFor, toCmdError } from "../../ipc/errors";
import { productLine } from "../../ipc/invoices";
import { useDraft } from "../../store/draft";
import { useUi } from "../../store/ui";
import { formatDecimal, numberLang } from "../../ui/number";
import { LineGrid } from "./LineGrid";
import { Answer, defaultTax, Question } from "./shared";

const fold = (s: string) =>
  s
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();

type Props = { products: Product[]; business: Business | undefined; currency: string };

/** Step 3: what is charged; catalogue tiles add lines, the grid adjusts them. */
export function StepLines({ products, business, currency }: Props) {
  const { t, i18n } = useTranslation();
  const lang = numberLang(i18n.language);
  const toast = useUi((s) => s.toast);
  const addLine = useDraft((s) => s.addLine);
  const [query, setQuery] = useState("");

  const addProduct = async (product: Product) => {
    const input = useDraft.getState().input;
    if (!input) return;
    try {
      addLine(await productLine(product.id, input.price_mode));
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t, "product"));
    }
  };

  /** A line of its own. Its texts land on the invoice, so they speak the invoice's language. */
  const addOwn = (kind: "gig" | "service" | "free" | "shipping" | "discount") => {
    const input = useDraft.getState().input;
    if (!input) return;
    const inv = { t: i18n.getFixedT(input.language) };
    const texts = {
      gig: { description: inv.t("Fee for the show"), unit: inv.t("flat rate") },
      service: { description: "", unit: inv.t("hours") },
      free: { description: "", unit: inv.t("piece") },
      shipping: { description: inv.t("Shipping"), unit: "" },
      discount: { description: inv.t("Discount"), unit: "" },
    }[kind];
    const follows = kind === "shipping" || kind === "discount";
    addLine({
      kind: follows ? kind : "free",
      product_id: null,
      ...texts,
      quantity: "1",
      unit_price: "0",
      tax: follows ? { mode: "follow_goods" } : defaultTax(business),
    });
  };

  const price = (p: Product) => {
    const negative = p.price.startsWith("-");
    return withCurrency(
      negative,
      formatDecimal(p.price.replace(/^-/, ""), lang, 2),
      currency,
      lang,
    );
  };

  const q = fold(query.trim());
  const shown = products.filter((p) => p.active && (!q || fold(p.name).includes(q)));

  return (
    <Question
      title={t("What are you charging for?")}
      sub={t("Each tile adds a line. You can adjust it afterwards.")}
    >
      {products.length > 0 && (
        <label className="search answer-search">
          <Search size={16} aria-hidden="true" />
          <input
            type="search"
            autoComplete="off"
            spellCheck={false}
            value={query}
            placeholder={t("Search the catalogue")}
            aria-label={t("Search the catalogue")}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
      )}
      <div className="answers compact">
        <Answer
          title={t("Gig fee")}
          sub={t("the show, as a flat rate")}
          icon={<Mic size={20} aria-hidden="true" />}
          onClick={() => addOwn("gig")}
        />
        <Answer
          title={t("Service")}
          sub={t("by the hour")}
          icon={<Clock size={20} aria-hidden="true" />}
          onClick={() => addOwn("service")}
        />
        {shown.map((p) => (
          <Answer
            key={p.id}
            title={p.name}
            icon={
              p.kind === "goods" ? (
                <Package size={20} aria-hidden="true" />
              ) : (
                <Clock size={20} aria-hidden="true" />
              )
            }
            sub={
              <>
                <span className="num">{price(p)}</span>
                {p.unit && ` / ${p.unit}`}
                {p.kind === "goods" && p.stock !== null && (
                  <>
                    {" · "}
                    {t("{count} in stock", { count: formatDecimal(p.stock, lang) })}
                  </>
                )}
              </>
            }
            onClick={() => void addProduct(p)}
          />
        ))}
        <Answer
          title={t("Free line")}
          sub={t("anything else")}
          icon={<PenLine size={20} aria-hidden="true" />}
          onClick={() => addOwn("free")}
        />
        <Answer
          title={t("Shipping")}
          sub={t("taxed like the goods")}
          icon={<Truck size={20} aria-hidden="true" />}
          onClick={() => addOwn("shipping")}
        />
        <Answer
          title={t("Discount")}
          sub={t("taken off the goods")}
          icon={<Percent size={20} aria-hidden="true" />}
          onClick={() => addOwn("discount")}
        />
      </div>
      <LineGrid business={business} currency={currency} />
    </Question>
  );
}
