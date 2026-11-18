import { type FormEvent, type ReactNode, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { NewProduct } from "../../ipc/bindings/NewProduct";
import type { PriceMode } from "../../ipc/bindings/PriceMode";
import type { Product } from "../../ipc/bindings/Product";
import type { ProductKind } from "../../ipc/bindings/ProductKind";
import type { TaxChoice } from "../../ipc/bindings/TaxChoice";
import { messageFor, toCmdError } from "../../ipc/errors";
import { createProduct, updateProduct } from "../../ipc/masterdata";
import { useUi } from "../../store/ui";
import { AmountField } from "../../ui/AmountField";
import { DecimalField } from "../../ui/DecimalField";
import { FormBar, saveOnCtrlS } from "../../ui/FormBar";
import { formatDecimal, numberLang } from "../../ui/number";
import { SelectField } from "../../ui/SelectField";
import { Switch } from "../../ui/Switch";
import { TextField } from "../../ui/TextField";

/** A tax choice as one value of the select field: "standard:19", "exempt", "follow_goods" … */
export function taxKey(tax: TaxChoice): string {
  if (tax.mode === "follow_goods") return "follow_goods";
  const c = tax.category;
  return c.type === "standard" ? `standard:${c.rate}` : c.type;
}

export function taxFromKey(key: string, reason: string): TaxChoice {
  if (key === "follow_goods") return { mode: "follow_goods" };
  if (key.startsWith("standard:"))
    return { mode: "fixed", category: { type: "standard", rate: key.slice("standard:".length) } };
  if (key === "exempt") return { mode: "fixed", category: { type: "exempt", reason } };
  const type = key as "small_business" | "reverse_charge" | "intra_community";
  return { mode: "fixed", category: { type } };
}

type Draft = Omit<NewProduct, "brand_id" | "tax"> & { tax: string; reason: string };

type Props = {
  product: Product | null;
  brandId: string;
  currency: string;
  /** The tax rates of the business the brand belongs to. */
  taxRates: string[];
  /** Price mode for a new product, from the brand. */
  priceMode?: PriceMode;
  onDone: (id: string) => void;
  onCancel: () => void;
  onDirty?: (dirty: boolean) => void;
  extra?: ReactNode;
};

/** An item in the catalog: goods with stock, or a service. */
export function ProductForm(props: Props) {
  const { product, brandId, currency, taxRates, onDone, onCancel, onDirty, extra } = props;
  const { t, i18n } = useTranslation();
  const lang = numberLang(i18n.language);
  const toast = useUi((s) => s.toast);
  const formId = useId();

  const initial = useRef<Draft>(
    product
      ? {
          name: product.name,
          description: product.description,
          kind: product.kind,
          unit: product.unit,
          price: product.price,
          price_mode: product.price_mode,
          tax: taxKey(product.tax),
          reason:
            product.tax.mode === "fixed" && product.tax.category.type === "exempt"
              ? product.tax.category.reason
              : "",
          stock: product.stock,
          active: product.active,
        }
      : {
          name: "",
          description: "",
          kind: "goods",
          unit: t("piece"),
          price: "",
          price_mode: props.priceMode ?? "net",
          tax: taxRates[0] ? `standard:${taxRates[0]}` : "exempt",
          reason: "",
          stock: null,
          active: true,
        },
  );
  const [draft, setDraft] = useState(initial.current);
  const [stockValid, setStockValid] = useState(true);
  const [busy, setBusy] = useState(false);

  const dirty = JSON.stringify(draft) !== JSON.stringify(initial.current);
  useEffect(() => onDirty?.(dirty), [dirty, onDirty]);

  const goods = draft.kind === "goods";
  const missing = [
    !draft.name.trim() && t("Name is missing"),
    draft.price === "" && t("Price is missing"),
    draft.tax === "exempt" && !draft.reason.trim() && t("Reason for the exemption is missing"),
    goods && !stockValid && t("Stock is not a number"),
  ].filter((m): m is string => Boolean(m));

  const set =
    <K extends keyof Draft>(key: K) =>
    (value: Draft[K]) =>
      setDraft((d) => ({ ...d, [key]: value }));

  // The business's rates, with the product's own rate kept even if the business dropped it.
  const rates = [...taxRates];
  const own = draft.tax.startsWith("standard:") ? draft.tax.slice("standard:".length) : null;
  if (own !== null) {
    const same = rates.findIndex((r) => Number(r) === Number(own));
    if (same >= 0) rates[same] = own;
    else rates.push(own);
  }
  const taxOptions = [
    ...rates.map((r) => ({ value: `standard:${r}`, label: `${formatDecimal(r, lang)} %` })),
    { value: "exempt", label: t("Tax exempt") },
    { value: "follow_goods", label: t("Like the goods (for shipping)") },
  ];
  const others: Record<string, string> = {
    small_business: t("Small business (§ 19 UStG)"),
    reverse_charge: t("Reverse charge"),
    intra_community: t("Intra-community supply"),
  };
  if (others[draft.tax]) taxOptions.push({ value: draft.tax, label: others[draft.tax] as string });

  const discard = () => {
    setDraft(initial.current);
    setStockValid(true);
    onCancel();
  };

  const save = async (e?: FormEvent) => {
    e?.preventDefault();
    if (missing.length > 0 || busy) return;
    setBusy(true);
    const { tax, reason, ...fields } = draft;
    const values = {
      ...fields,
      tax: taxFromKey(tax, reason.trim()),
      stock: goods && fields.stock ? fields.stock : null,
    };
    try {
      if (product) {
        await updateProduct({ ...product, ...values });
        onDone(product.id);
      } else {
        onDone(await createProduct({ ...values, brand_id: brandId }));
      }
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t, "product"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form id={formId} className="edit-form" onSubmit={save} onKeyDown={saveOnCtrlS}>
      <h2 className="form-title">{product ? product.name || t("Item") : t("New item")}</h2>

      <section className="form-section">
        <h3>{t("Item")}</h3>
        <div className="field-grid">
          <SelectField<ProductKind>
            label={t("Kind")}
            value={draft.kind}
            onChange={set("kind")}
            options={[
              { value: "goods", label: t("Goods") },
              { value: "service", label: t("Service") },
            ]}
          />
          <TextField
            label={t("Unit")}
            value={draft.unit}
            onChange={set("unit")}
            hint={t("As on the invoice, such as {examples}.", {
              examples: goods ? t("piece, box") : t("hour, flat rate"),
            })}
          />
          <TextField label={t("Name")} value={draft.name} onChange={set("name")} wide />
          <TextField
            label={t("Description")}
            value={draft.description}
            onChange={set("description")}
            hint={t("Printed under the name on the invoice.")}
            multiline
          />
        </div>
      </section>

      <section className="form-section">
        <h3>{t("Price and tax")}</h3>
        <div className="field-grid">
          <AmountField
            label={t("Price")}
            value={draft.price}
            onChange={(v) => set("price")(v)}
            currency={currency}
          />
          <SelectField<PriceMode>
            label={t("Price is")}
            value={draft.price_mode}
            onChange={set("price_mode")}
            options={[
              { value: "net", label: t("Net, tax is added") },
              { value: "gross", label: t("Gross, tax is included") },
            ]}
          />
          <SelectField
            label={t("Tax")}
            value={draft.tax}
            onChange={set("tax")}
            options={taxOptions}
          />
          {draft.tax === "exempt" && (
            <TextField
              label={t("Reason for the exemption")}
              value={draft.reason}
              onChange={set("reason")}
              hint={t("Printed on the invoice, such as the paragraph of the law.")}
            />
          )}
        </div>
      </section>

      <section className="form-section">
        <h3>{t("Availability")}</h3>
        <div className="field-grid">
          {goods && (
            <DecimalField
              label={t("Stock")}
              value={draft.stock ?? ""}
              unit={draft.unit}
              onChange={(v, valid) => {
                setStockValid(valid);
                set("stock")(v === "" ? null : v);
              }}
              hint={t("Leave empty if the stock is not counted. Issued invoices count it down.")}
            />
          )}
          <div className="field wide">
            <Switch
              label={t("Active, can be picked for invoices")}
              checked={draft.active}
              onChange={set("active")}
            />
          </div>
        </div>
      </section>

      <FormBar
        formId={formId}
        missing={missing}
        busy={busy}
        dirty={dirty}
        onDiscard={discard}
        extra={extra}
      />
    </form>
  );
}
