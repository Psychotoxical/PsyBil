import { type FormEvent, type ReactNode, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { LANGUAGES } from "../../i18n";
import type { Customer } from "../../ipc/bindings/Customer";
import type { CustomerKind } from "../../ipc/bindings/CustomerKind";
import type { NewCustomer } from "../../ipc/bindings/NewCustomer";
import { messageFor, toCmdError } from "../../ipc/errors";
import { createCustomer, updateCustomer } from "../../ipc/masterdata";
import { useUi } from "../../store/ui";
import { FormBar, saveOnCtrlS } from "../../ui/FormBar";
import { SelectField } from "../../ui/SelectField";
import { TextField } from "../../ui/TextField";

const NEW_CUSTOMER: NewCustomer = {
  name: "",
  street: "",
  zip: "",
  city: "",
  country: "DE",
  kind: "business",
  vat_id: "",
  email: "",
  phone: "",
  notes: "",
  language: "de",
};

function toDraft(customer: Customer | null): NewCustomer {
  if (!customer) return { ...NEW_CUSTOMER };
  const { id: _id, archived: _archived, ...fields } = customer;
  return fields;
}

type Props = {
  customer: Customer | null;
  onDone: (id: string) => void;
  onCancel: () => void;
  onDirty?: (dirty: boolean) => void;
  extra?: ReactNode;
  /** False inside a dialog that already carries the title. */
  showTitle?: boolean;
};

/** Who an invoice goes to. */
export function CustomerForm({
  customer,
  onDone,
  onCancel,
  onDirty,
  extra,
  showTitle = true,
}: Props) {
  const { t } = useTranslation();
  const toast = useUi((s) => s.toast);
  const formId = useId();
  const initial = useRef(toDraft(customer));
  const [draft, setDraft] = useState(initial.current);
  const [busy, setBusy] = useState(false);

  const dirty = JSON.stringify(draft) !== JSON.stringify(initial.current);
  useEffect(() => onDirty?.(dirty), [dirty, onDirty]);

  const missing = [!draft.name.trim() && t("Name is missing")].filter((m): m is string =>
    Boolean(m),
  );

  const set =
    <K extends keyof NewCustomer>(key: K) =>
    (value: NewCustomer[K]) =>
      setDraft((d) => ({ ...d, [key]: value }));

  const discard = () => {
    setDraft(initial.current);
    onCancel();
  };

  const save = async (e?: FormEvent) => {
    e?.preventDefault();
    if (missing.length > 0 || busy) return;
    setBusy(true);
    const input = { ...draft, country: draft.country.trim().toUpperCase() };
    try {
      if (customer) {
        await updateCustomer({ ...customer, ...input });
        onDone(customer.id);
      } else {
        onDone(await createCustomer(input));
      }
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t, "customer"));
    } finally {
      setBusy(false);
    }
  };

  const kinds: { value: CustomerKind; label: string }[] = [
    { value: "business", label: t("Company") },
    { value: "private", label: t("Private person") },
  ];
  if (draft.kind === "unknown") kinds.push({ value: "unknown", label: t("Not known") });

  return (
    <form id={formId} className="edit-form" onSubmit={save} onKeyDown={saveOnCtrlS}>
      {showTitle && (
        <h2 className="form-title">
          {customer ? customer.name || t("Customer") : t("New customer")}
        </h2>
      )}

      <section className="form-section">
        <h3>{t("Address")}</h3>
        <div className="field-grid">
          <SelectField
            label={t("Kind")}
            value={draft.kind}
            onChange={set("kind")}
            options={kinds}
          />
          <span aria-hidden="true" />
          <TextField label={t("Name")} value={draft.name} onChange={set("name")} wide />
          <TextField label={t("Street")} value={draft.street} onChange={set("street")} wide />
          <TextField label={t("Postcode")} value={draft.zip} onChange={set("zip")} mono />
          <TextField label={t("City")} value={draft.city} onChange={set("city")} />
          <TextField
            label={t("Country")}
            value={draft.country}
            onChange={set("country")}
            hint={t("Two letters, as in {example}.", { example: "DE" })}
            maxLength={2}
            mono
          />
          {draft.kind !== "private" && (
            <TextField
              label={t("VAT ID")}
              value={draft.vat_id}
              onChange={set("vat_id")}
              hint={t("Needed for customers in other EU countries.")}
              mono
            />
          )}
        </div>
      </section>

      <section className="form-section">
        <h3>{t("Contact")}</h3>
        <div className="field-grid">
          <TextField label={t("Email")} type="email" value={draft.email} onChange={set("email")} />
          <TextField label={t("Phone")} type="tel" value={draft.phone} onChange={set("phone")} />
          <SelectField
            label={t("Invoice language")}
            value={draft.language}
            onChange={set("language")}
            options={LANGUAGES.map(([value, label]) => ({ value, label }))}
          />
          <TextField
            label={t("Note")}
            value={draft.notes}
            onChange={set("notes")}
            hint={t("Only for you; it is not printed.")}
            multiline
          />
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
