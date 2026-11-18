import "./senders.css";
import { Archive, ArchiveRestore, Building2, Plus, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Brand } from "../../ipc/bindings/Brand";
import type { Business } from "../../ipc/bindings/Business";
import { messageFor, type Subject, toCmdError } from "../../ipc/errors";
import {
  archiveBrand,
  archiveBusiness,
  deleteBrand,
  deleteBusiness,
  listBrands,
  listBusinesses,
} from "../../ipc/masterdata";
import { ConfirmDialog } from "../../shell/ConfirmDialog";
import { PageHeader } from "../../shell/PageHeader";
import { useUi, useUnsavedGuard } from "../../store/ui";
import { Switch } from "../../ui/Switch";
import { BrandForm } from "./BrandForm";
import { BusinessForm } from "./BusinessForm";

/** What the form on the right shows; `id` null is a new one. */
type Selection =
  | { kind: "business"; id: string | null }
  | { kind: "brand"; id: string | null; businessId: string };

type Pending =
  | { kind: "switch"; to: Selection }
  | { kind: "delete"; subject: "business" | "brand"; id: string; name: string };

const keyOf = (s: Selection) => `${s.kind}:${s.id ?? "new"}`;

/** Businesses and their brands: who is on the invoice. */
export function Senders() {
  const { t } = useTranslation();
  const toast = useUi((s) => s.toast);
  const bump = useUi((s) => s.bump);
  const [showArchived, setShowArchived] = useState(false);
  const [businesses, setBusinesses] = useState<Business[] | null>(null);
  const [brands, setBrands] = useState<Record<string, Brand[]>>({});
  const [selection, setSelection] = useState<Selection | null>(null);
  const [dirty, setDirty] = useState(false);
  useUnsavedGuard(dirty);
  const [pending, setPending] = useState<Pending | null>(null);

  const fail = useCallback(
    (error: unknown, subject?: Subject) =>
      toast("error", messageFor(toCmdError(error), t, subject)),
    [toast, t],
  );

  const reload = useCallback(async () => {
    try {
      const all = await listBusinesses(showArchived);
      const lists = await Promise.all(all.map((b) => listBrands(b.id)));
      setBusinesses(all);
      setBrands(Object.fromEntries(all.map((b, i) => [b.id, lists[i] ?? []])));
      setSelection((s) => s ?? (all[0] ? { kind: "business", id: all[0].id } : null));
    } catch (error) {
      fail(error);
    }
  }, [showArchived, fail]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const select = (to: Selection) => {
    if (selection && keyOf(to) === keyOf(selection)) return;
    if (dirty) setPending({ kind: "switch", to });
    else setSelection(to);
  };

  const saved = (to: Selection) => {
    setDirty(false);
    setSelection(to);
    toast("info", t("Saved."));
    bump();
    void reload();
  };

  const archive = async (subject: "business" | "brand", id: string, archived: boolean) => {
    try {
      if (subject === "business") await archiveBusiness(id, archived);
      else await archiveBrand(id, archived);
      // An archived business leaves the list unless archived ones are shown.
      if (archived && subject === "business" && !showArchived) setSelection(null);
      bump();
      await reload();
    } catch (error) {
      fail(error, subject);
    }
  };

  const remove = async (subject: "business" | "brand", id: string) => {
    try {
      if (subject === "business") await deleteBusiness(id);
      else await deleteBrand(id);
      setDirty(false);
      setSelection(null);
      bump();
      await reload();
    } catch (error) {
      fail(error, subject);
    }
  };

  const allBrands = Object.values(brands).flat();
  const business =
    selection?.kind === "business" && selection.id
      ? businesses?.find((b) => b.id === selection.id)
      : undefined;
  const brand =
    selection?.kind === "brand" && selection.id
      ? allBrands.find((b) => b.id === selection.id)
      : undefined;

  const objectActions = (
    subject: "business" | "brand",
    item: { id: string; name: string; archived: boolean },
  ) => (
    <>
      <button
        type="button"
        className="btn ghost"
        onClick={() => archive(subject, item.id, !item.archived)}
      >
        {item.archived ? (
          <ArchiveRestore size={16} aria-hidden="true" />
        ) : (
          <Archive size={16} aria-hidden="true" />
        )}
        {item.archived ? t("Restore") : t("Archive")}
      </button>
      <button
        type="button"
        className="btn ghost danger-text"
        onClick={() => setPending({ kind: "delete", subject, id: item.id, name: item.name })}
      >
        <Trash2 size={16} aria-hidden="true" />
        {t("Delete")}
      </button>
    </>
  );

  const form = (() => {
    if (!selection) return null;
    const common = { onDirty: setDirty };
    if (selection.kind === "business") {
      if (selection.id && !business) return null;
      return (
        <BusinessForm
          key={keyOf(selection)}
          business={business ?? null}
          onDone={(id) => saved({ kind: "business", id })}
          extra={business && objectActions("business", business)}
          {...common}
          onCancel={() => {
            if (!business) setSelection(null);
          }}
        />
      );
    }
    if (selection.id && !brand) return null;
    return (
      <BrandForm
        key={keyOf(selection)}
        brand={brand ?? null}
        businessId={selection.businessId}
        onDone={(id) => saved({ kind: "brand", id, businessId: selection.businessId })}
        extra={brand && objectActions("brand", brand)}
        {...common}
        onCancel={() => {
          if (!brand) setSelection({ kind: "business", id: selection.businessId });
        }}
      />
    );
  })();

  const current = selection ? keyOf(selection) : "";

  return (
    <>
      <PageHeader
        title={t("Senders")}
        sub={t(
          "Who is on the invoice: the business that owes the taxes, and the brands it appears as.",
        )}
        actions={
          <button
            type="button"
            className="btn"
            onClick={() => select({ kind: "business", id: null })}
          >
            <Plus size={16} aria-hidden="true" />
            {t("Add business")}
          </button>
        }
      />

      {businesses && businesses.length === 0 && !selection ? (
        <div className="empty">
          <h2>{t("No sender yet")}</h2>
          <p>{t("Add your business first. Its name and address appear on every invoice.")}</p>
          <button
            type="button"
            className="btn primary"
            onClick={() => select({ kind: "business", id: null })}
          >
            <Plus size={16} aria-hidden="true" />
            {t("Add business")}
          </button>
        </div>
      ) : (
        <div className="split">
          <nav className="split-list" aria-label={t("Businesses and brands")}>
            {businesses?.map((b) => (
              <div className="split-group" key={b.id}>
                <button
                  type="button"
                  className="split-item business"
                  aria-current={current === `business:${b.id}` ? "true" : undefined}
                  onClick={() => select({ kind: "business", id: b.id })}
                >
                  <Building2 size={16} aria-hidden="true" />
                  <span className="split-item-name">{b.name}</span>
                  {b.archived && <span className="tag">{t("Archived")}</span>}
                </button>
                <ul>
                  {(brands[b.id] ?? [])
                    .filter((br) => showArchived || !br.archived)
                    .map((br) => (
                      <li key={br.id}>
                        <button
                          type="button"
                          className="split-item brand"
                          aria-current={current === `brand:${br.id}` ? "true" : undefined}
                          onClick={() => select({ kind: "brand", id: br.id, businessId: b.id })}
                        >
                          <span
                            className="brand-swatch"
                            style={{ background: br.color }}
                            aria-hidden="true"
                          />
                          <span className="split-item-name">{br.name}</span>
                          {br.archived && <span className="tag">{t("Archived")}</span>}
                        </button>
                      </li>
                    ))}
                  <li>
                    <button
                      type="button"
                      className="split-item add"
                      aria-current={
                        selection?.kind === "brand" &&
                        !selection.id &&
                        selection.businessId === b.id
                          ? "true"
                          : undefined
                      }
                      onClick={() => select({ kind: "brand", id: null, businessId: b.id })}
                    >
                      <Plus size={14} aria-hidden="true" />
                      {t("Add brand")}
                    </button>
                  </li>
                </ul>
              </div>
            ))}
            <div className="split-list-foot">
              <Switch
                label={t("Show archived")}
                checked={showArchived}
                onChange={setShowArchived}
              />
            </div>
          </nav>
          <div className="split-detail">{form}</div>
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
            setSelection(pending.to);
            setPending(null);
          }}
        />
      )}
      {pending?.kind === "delete" && (
        <ConfirmDialog
          title={t("Delete {name}?", { name: pending.name })}
          text={t("This cannot be undone. Archiving keeps it for later instead.")}
          confirmLabel={t("Delete")}
          danger
          onCancel={() => setPending(null)}
          onConfirm={() => {
            setPending(null);
            void remove(pending.subject, pending.id);
          }}
        />
      )}
    </>
  );
}
