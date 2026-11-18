import { Plus, Search, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { withCurrency } from "../../format";
import type { Brand } from "../../ipc/bindings/Brand";
import type { Business } from "../../ipc/bindings/Business";
import type { Product } from "../../ipc/bindings/Product";
import { messageFor, toCmdError } from "../../ipc/errors";
import { deleteProduct, listBrands, listBusinesses, listProducts } from "../../ipc/masterdata";
import { ConfirmDialog } from "../../shell/ConfirmDialog";
import { PageHeader } from "../../shell/PageHeader";
import { useUi, useUnsavedGuard } from "../../store/ui";
import { formatDecimal, numberLang } from "../../ui/number";
import { SelectField } from "../../ui/SelectField";
import { Switch } from "../../ui/Switch";
import { ProductForm } from "./ProductForm";

type Selection = string | "new" | null;
type Pending =
  | { kind: "switch"; to: Selection; brand?: string }
  | { kind: "delete"; product: Product };
type BrandOf = { brand: Brand; business: Business };

const fold = (s: string) =>
  s
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();

/** Goods and services per brand, to pick from on an invoice. */
export function Catalog() {
  const { t, i18n } = useTranslation();
  const lang = numberLang(i18n.language);
  const toast = useUi((s) => s.toast);
  const [brands, setBrands] = useState<BrandOf[] | null>(null);
  const [brandId, setBrandId] = useState<string | null>(null);
  const [products, setProducts] = useState<Product[] | null>(null);
  const [showInactive, setShowInactive] = useState(false);
  const [query, setQuery] = useState("");
  const [selection, setSelection] = useState<Selection>(null);
  const [dirty, setDirty] = useState(false);
  useUnsavedGuard(dirty);
  const [pending, setPending] = useState<Pending | null>(null);

  const fail = useCallback(
    (error: unknown) => toast("error", messageFor(toCmdError(error), t, "product")),
    [toast, t],
  );

  useEffect(() => {
    (async () => {
      const businesses = await listBusinesses(false);
      const lists = await Promise.all(businesses.map((b) => listBrands(b.id)));
      const all = businesses.flatMap((business, i) =>
        (lists[i] ?? []).filter((b) => !b.archived).map((brand) => ({ brand, business })),
      );
      setBrands(all);
      setBrandId((id) => id ?? all[0]?.brand.id ?? null);
    })().catch(fail);
  }, [fail]);

  const reload = useCallback(async () => {
    if (!brandId) return;
    try {
      setProducts(await listProducts(brandId, showInactive));
    } catch (error) {
      fail(error);
    }
  }, [brandId, showInactive, fail]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const select = (to: Selection, brand?: string) => {
    if (to === selection && (brand === undefined || brand === brandId)) return;
    if (dirty) {
      setPending({ kind: "switch", to, brand });
      return;
    }
    if (brand !== undefined) setBrandId(brand);
    setSelection(to);
  };

  const current = brands?.find((b) => b.brand.id === brandId);
  const currency = current?.business.currency ?? "EUR";
  const product = products?.find((p) => p.id === selection);
  const q = fold(query.trim());
  const shown = (products ?? []).filter(
    (p) => !q || fold(`${p.name} ${p.description}`).includes(q),
  );

  const price = (p: Product) => {
    const negative = p.price.startsWith("-");
    const number = formatDecimal(p.price.replace(/^-/, ""), lang, 2);
    return withCurrency(negative, number, currency, lang);
  };
  const sub = (p: Product) => (
    <>
      <span className="num">{price(p)}</span>
      {p.unit && ` / ${p.unit}`}
      {p.kind === "goods" &&
        p.stock !== null &&
        ` · ${t("{count} in stock", { count: formatDecimal(p.stock, lang) })}`}
    </>
  );

  return (
    <>
      <PageHeader
        title={t("Catalogue")}
        sub={t("Goods and services you invoice often, with price and tax.")}
        actions={
          <button type="button" className="btn" disabled={!brandId} onClick={() => select("new")}>
            <Plus size={16} aria-hidden="true" />
            {t("New item")}
          </button>
        }
      />
      {brands && brands.length === 0 ? (
        <div className="empty">
          <h2>{t("No sender yet")}</h2>
          <p>{t("Items belong to a brand. Add your business under Senders first.")}</p>
        </div>
      ) : (
        <div className="split">
          <nav className="split-list" aria-label={t("Catalogue")}>
            {brands && brands.length > 1 && brandId && (
              <SelectField
                label={t("Brand")}
                value={brandId}
                onChange={(id) => select(null, id)}
                options={brands.map(({ brand, business }) => ({
                  value: brand.id,
                  label:
                    brand.name === business.name ? brand.name : `${business.name} · ${brand.name}`,
                }))}
              />
            )}
            <label className="search">
              <Search size={16} aria-hidden="true" />
              <input
                type="search"
                autoComplete="off"
                spellCheck={false}
                value={query}
                placeholder={t("Search")}
                aria-label={t("Search items")}
                onChange={(e) => setQuery(e.target.value)}
              />
            </label>
            {products && shown.length === 0 && (
              <p className="split-empty">
                {products.length === 0 ? t("No items yet.") : t("Nothing found.")}
              </p>
            )}
            <ul>
              {shown.map((p) => (
                <li key={p.id}>
                  <button
                    type="button"
                    className="split-item two-line"
                    aria-current={selection === p.id ? "true" : undefined}
                    onClick={() => select(p.id)}
                  >
                    <span className="split-item-text">
                      <span className="split-item-name">{p.name}</span>
                      <span className="split-item-sub">{sub(p)}</span>
                    </span>
                    {!p.active && <span className="tag">{t("Off")}</span>}
                  </button>
                </li>
              ))}
            </ul>
            <div className="split-list-foot">
              <Switch
                label={t("Show switched off")}
                checked={showInactive}
                onChange={setShowInactive}
              />
            </div>
          </nav>

          <div className="split-detail">
            {selection === null || (selection !== "new" && !product) || !brandId ? (
              <div className="empty">
                <h2>{t("Pick an item")}</h2>
                <p>{t("Choose one on the left to change it, or add a new one.")}</p>
                <button
                  type="button"
                  className="btn primary"
                  disabled={!brandId}
                  onClick={() => select("new")}
                >
                  <Plus size={16} aria-hidden="true" />
                  {t("New item")}
                </button>
              </div>
            ) : (
              <ProductForm
                key={`${brandId}:${selection}`}
                product={product ?? null}
                brandId={brandId}
                currency={currency}
                taxRates={current?.business.tax_rates ?? []}
                priceMode={current?.brand.price_mode}
                onDirty={setDirty}
                onCancel={() => {
                  if (!product) setSelection(null);
                }}
                onDone={(id) => {
                  toast("info", t("Saved."));
                  setDirty(false);
                  setSelection(id);
                  void reload();
                }}
                extra={
                  product && (
                    <button
                      type="button"
                      className="btn ghost danger-text"
                      onClick={() => setPending({ kind: "delete", product })}
                    >
                      <Trash2 size={16} aria-hidden="true" />
                      {t("Delete")}
                    </button>
                  )
                }
              />
            )}
          </div>
        </div>
      )}

      {pending?.kind === "switch" && (
        <ConfirmDialog
          title={t("Discard changes?")}
          text={t("The changes in this form are not saved yet.")}
          confirmLabel={t("Discard")}
          onCancel={() => setPending(null)}
          onConfirm={() => {
            setDirty(false);
            if (pending.brand !== undefined) setBrandId(pending.brand);
            setSelection(pending.to);
            setPending(null);
          }}
        />
      )}
      {pending?.kind === "delete" && (
        <ConfirmDialog
          title={t("Delete {name}?", { name: pending.product.name })}
          text={t("This cannot be undone. Switching it off keeps it for later instead.")}
          confirmLabel={t("Delete")}
          danger
          onCancel={() => setPending(null)}
          onConfirm={async () => {
            setPending(null);
            try {
              await deleteProduct(pending.product.id);
              setDirty(false);
              setSelection(null);
              await reload();
            } catch (error) {
              fail(error);
            }
          }}
        />
      )}
    </>
  );
}
