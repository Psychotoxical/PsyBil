import { Archive, ArchiveRestore, Plus, Search, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Customer } from "../../ipc/bindings/Customer";
import { messageFor, toCmdError } from "../../ipc/errors";
import { archiveCustomer, deleteCustomer, listCustomers } from "../../ipc/masterdata";
import { ConfirmDialog } from "../../shell/ConfirmDialog";
import { PageHeader } from "../../shell/PageHeader";
import { useUi, useUnsavedGuard } from "../../store/ui";
import { Switch } from "../../ui/Switch";
import { CustomerForm } from "./CustomerForm";

/** The selected customer; "new" while one is being added. */
type Selection = string | "new" | null;

type Pending = { kind: "switch"; to: Selection } | { kind: "delete"; customer: Customer };

/** Lower case without accents, for searching. */
const fold = (s: string) =>
  s
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();

function matches(customer: Customer, query: string): boolean {
  const q = fold(query.trim());
  if (!q) return true;
  return [customer.name, customer.city, customer.email, customer.zip].some((v) =>
    fold(v).includes(q),
  );
}

/** Everyone an invoice can go to. */
export function Customers() {
  const { t } = useTranslation();
  const toast = useUi((s) => s.toast);
  const [customers, setCustomers] = useState<Customer[] | null>(null);
  const [showArchived, setShowArchived] = useState(false);
  const [query, setQuery] = useState("");
  const [selection, setSelection] = useState<Selection>(null);
  const [dirty, setDirty] = useState(false);
  useUnsavedGuard(dirty);
  const [pending, setPending] = useState<Pending | null>(null);

  const reload = useCallback(async () => {
    try {
      setCustomers(await listCustomers(showArchived));
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t, "customer"));
    }
  }, [showArchived, toast, t]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const select = (to: Selection) => {
    if (to === selection) return;
    if (dirty) setPending({ kind: "switch", to });
    else setSelection(to);
  };

  const run = async (action: () => Promise<void>, after: Selection) => {
    try {
      await action();
      setDirty(false);
      setSelection(after);
      await reload();
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t, "customer"));
    }
  };

  const shown = (customers ?? []).filter((c) => matches(c, query));
  const current = customers?.find((c) => c.id === selection);
  const kindLabel = (c: Customer) =>
    c.kind === "business" ? t("Company") : c.kind === "private" ? t("Private person") : "";

  return (
    <>
      <PageHeader
        title={t("Customers")}
        sub={t("Who your invoices go to. Saved details fill in the address on every new invoice.")}
        actions={
          <button type="button" className="btn" onClick={() => select("new")}>
            <Plus size={16} aria-hidden="true" />
            {t("New customer")}
          </button>
        }
      />
      <div className="split">
        <nav className="split-list" aria-label={t("Customers")}>
          <label className="search">
            <Search size={16} aria-hidden="true" />
            <input
              type="search"
              autoComplete="off"
              spellCheck={false}
              value={query}
              placeholder={t("Search")}
              aria-label={t("Search customers")}
              onChange={(e) => setQuery(e.target.value)}
            />
          </label>
          {customers && shown.length === 0 && (
            <p className="split-empty">
              {customers.length === 0 ? t("No customers yet.") : t("Nothing found.")}
            </p>
          )}
          <ul>
            {shown.map((c) => (
              <li key={c.id}>
                <button
                  type="button"
                  className="split-item two-line"
                  aria-current={selection === c.id ? "true" : undefined}
                  onClick={() => select(c.id)}
                >
                  <span className="split-item-text">
                    <span className="split-item-name">{c.name}</span>
                    <span className="split-item-sub">
                      {[c.city, kindLabel(c)].filter(Boolean).join(" · ")}
                    </span>
                  </span>
                  {c.archived && <span className="tag">{t("Archived")}</span>}
                </button>
              </li>
            ))}
          </ul>
          <div className="split-list-foot">
            <Switch label={t("Show archived")} checked={showArchived} onChange={setShowArchived} />
          </div>
        </nav>

        <div className="split-detail">
          {selection === null || (selection !== "new" && !current) ? (
            <div className="empty">
              <h2>{t("Pick a customer")}</h2>
              <p>{t("Choose one on the left to see and change the details, or add a new one.")}</p>
              <button type="button" className="btn primary" onClick={() => select("new")}>
                <Plus size={16} aria-hidden="true" />
                {t("New customer")}
              </button>
            </div>
          ) : (
            <CustomerForm
              key={selection}
              customer={current ?? null}
              onDirty={setDirty}
              onCancel={() => {
                if (!current) setSelection(null);
              }}
              onDone={(id) => {
                toast("info", t("Saved."));
                void run(async () => {}, id);
              }}
              extra={
                current && (
                  <>
                    <button
                      type="button"
                      className="btn ghost"
                      onClick={() =>
                        run(
                          () => archiveCustomer(current.id, !current.archived),
                          !current.archived && !showArchived ? null : current.id,
                        )
                      }
                    >
                      {current.archived ? (
                        <ArchiveRestore size={16} aria-hidden="true" />
                      ) : (
                        <Archive size={16} aria-hidden="true" />
                      )}
                      {current.archived ? t("Restore") : t("Archive")}
                    </button>
                    <button
                      type="button"
                      className="btn ghost danger-text"
                      onClick={() => setPending({ kind: "delete", customer: current })}
                    >
                      <Trash2 size={16} aria-hidden="true" />
                      {t("Delete")}
                    </button>
                  </>
                )
              }
            />
          )}
        </div>
      </div>

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
          title={t("Delete {name}?", { name: pending.customer.name })}
          text={t("This cannot be undone. Archiving keeps it for later instead.")}
          confirmLabel={t("Delete")}
          danger
          onCancel={() => setPending(null)}
          onConfirm={() => {
            setPending(null);
            void run(() => deleteCustomer(pending.customer.id), null);
          }}
        />
      )}
    </>
  );
}
