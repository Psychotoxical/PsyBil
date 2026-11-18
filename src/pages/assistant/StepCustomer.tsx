import { Building2, Pencil, Plus, Search, User } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Customer } from "../../ipc/bindings/Customer";
import { Dialog } from "../../shell/Dialog";
import { useDraft } from "../../store/draft";
import { CustomerForm } from "../customers/CustomerForm";
import { Answer, Question } from "./shared";

const fold = (s: string) =>
  s
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();

type Props = {
  customers: Customer[];
  /** Loads the customers again and returns them. */
  reload: () => Promise<Customer[]>;
  onDone: () => void;
};

/** Step 2: who gets the invoice; a new customer can be added on the spot. */
export function StepCustomer({ customers, reload, onDone }: Props) {
  const { t } = useTranslation();
  const input = useDraft((s) => s.input);
  const change = useDraft((s) => s.change);
  const [query, setQuery] = useState("");
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState(false);
  const chosen = customers.find((c) => c.id === input?.customer_id);

  const pick = (customer: Customer) => {
    const current = useDraft.getState().input;
    if (!current) return;
    change({
      ...current,
      customer_id: customer.id,
      language: customer.language || current.language,
    });
    onDone();
  };

  const q = fold(query.trim());
  const shown = customers
    .filter((c) => !c.archived)
    .filter((c) => !q || fold(`${c.name} ${c.city}`).includes(q));

  return (
    <Question title={t("Who gets the invoice?")} sub={t("Type a name or add a new customer.")}>
      <label className="search answer-search">
        <Search size={16} aria-hidden="true" />
        <input
          type="search"
          autoComplete="off"
          spellCheck={false}
          value={query}
          placeholder={t("Name or city")}
          aria-label={t("Search customers")}
          onChange={(e) => setQuery(e.target.value)}
        />
      </label>
      <div className="answers">
        {shown.map((c) => (
          <Answer
            key={c.id}
            title={c.name}
            sub={c.city || undefined}
            icon={
              c.kind === "private" ? (
                <User size={22} aria-hidden="true" />
              ) : (
                <Building2 size={22} aria-hidden="true" />
              )
            }
            pressed={input?.customer_id === c.id}
            onClick={() => pick(c)}
          />
        ))}
        <button type="button" className="answer add" onClick={() => setAdding(true)}>
          <span className="answer-icon">
            <Plus size={22} aria-hidden="true" />
          </span>
          <span className="answer-title">{t("Add new customer")}</span>
        </button>
      </div>
      {chosen && (
        <p className="answer-note">
          <button type="button" className="btn ghost" onClick={() => setEditing(true)}>
            <Pencil size={16} aria-hidden="true" />
            {t("Edit details of {name}", { name: chosen.name })}
          </button>
        </p>
      )}
      {editing && chosen && (
        <Dialog title={chosen.name} onClose={() => setEditing(false)} wide>
          <CustomerForm
            customer={chosen}
            showTitle={false}
            onCancel={() => setEditing(false)}
            onDone={async () => {
              await reload();
              setEditing(false);
            }}
          />
        </Dialog>
      )}
      {adding && (
        <Dialog title={t("New customer")} onClose={() => setAdding(false)} wide>
          <CustomerForm
            customer={null}
            showTitle={false}
            onCancel={() => setAdding(false)}
            onDone={async (id) => {
              const all = await reload();
              const created = all.find((c) => c.id === id);
              setAdding(false);
              if (created) pick(created);
            }}
          />
        </Dialog>
      )}
    </Question>
  );
}
