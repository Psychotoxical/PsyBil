import { open } from "@tauri-apps/plugin-dialog";
import { ImagePlus } from "lucide-react";
import { type FormEvent, type ReactNode, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Brand } from "../../ipc/bindings/Brand";
import type { PriceMode } from "../../ipc/bindings/PriceMode";
import { messageFor, toCmdError } from "../../ipc/errors";
import {
  brandLogo,
  createBrand,
  removeBrandLogo,
  setBrandLogo,
  updateBrand,
} from "../../ipc/masterdata";
import { useUi } from "../../store/ui";
import { ColorField, HEX_COLOR } from "../../ui/ColorField";
import { FormBar, saveOnCtrlS } from "../../ui/FormBar";
import { SelectField } from "../../ui/SelectField";
import { TextField } from "../../ui/TextField";

/** The fields the form edits; id, business, logo and archive state stay as they are. */
type Draft = Pick<
  Brand,
  | "name"
  | "color"
  | "phone"
  | "email"
  | "website"
  | "bank_name"
  | "iban"
  | "bic"
  | "pdf_template"
  | "invoice_prefix"
  | "intro_text"
  | "outro_text"
  | "price_mode"
>;

/** The same defaults as `NewBrand::default` in the core. */
const NEW_BRAND: Draft = {
  name: "",
  color: "#8839ef",
  phone: "",
  email: "",
  website: "",
  bank_name: "",
  iban: "",
  bic: "",
  pdf_template: "classic",
  invoice_prefix: "RE",
  intro_text: "",
  outro_text: "",
  price_mode: "net",
};

function toDraft(brand: Brand | null): Draft {
  if (!brand) return { ...NEW_BRAND };
  const draft = { ...NEW_BRAND };
  for (const key of Object.keys(NEW_BRAND) as (keyof Draft)[]) {
    (draft as Record<string, unknown>)[key] = brand[key];
  }
  return draft;
}

/** A logo change waits for "Save", like every other field. */
type LogoChange = { kind: "set"; path: string } | { kind: "remove" } | null;

type Props = {
  /** The brand to edit, or null for a new one. */
  brand: Brand | null;
  businessId: string;
  onDone: (id: string) => void;
  onCancel: () => void;
  onDirty?: (dirty: boolean) => void;
  extra?: ReactNode;
};

/** How a business appears on its invoices: name, logo, colour, contact, bank, texts. */
export function BrandForm({ brand, businessId, onDone, onCancel, onDirty, extra }: Props) {
  const { t } = useTranslation();
  const toast = useUi((s) => s.toast);
  const formId = useId();
  const initial = useRef(toDraft(brand));
  const [draft, setDraft] = useState(initial.current);
  const [days, setDays] = useState(String(brand?.payment_days ?? 14));
  const [logo, setLogo] = useState<LogoChange>(null);
  const [logoUrl, setLogoUrl] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!brand?.logo_mime) return;
    let live = true;
    brandLogo(brand.id)
      .then((url) => live && setLogoUrl(url))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [brand]);

  const paymentDays = /^\d{1,3}$/.test(days.trim()) ? Number(days) : Number.NaN;
  const dirty =
    logo !== null ||
    String(brand?.payment_days ?? 14) !== days ||
    JSON.stringify(draft) !== JSON.stringify(initial.current);
  useEffect(() => onDirty?.(dirty), [dirty, onDirty]);

  const missing = [
    !draft.name.trim() && t("Name is missing"),
    !HEX_COLOR.test(draft.color) && t("Colour is not valid"),
    !(paymentDays >= 0 && paymentDays <= 365) && t("Payment term is not valid"),
  ].filter((m): m is string => Boolean(m));

  const set =
    <K extends keyof Draft>(key: K) =>
    (value: Draft[K]) =>
      setDraft((d) => ({ ...d, [key]: value }));

  const pick = async () => {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: t("Image"), extensions: ["png", "jpg", "jpeg"] }],
    });
    if (typeof path === "string") setLogo({ kind: "set", path });
  };

  const discard = () => {
    setDraft(initial.current);
    setDays(String(brand?.payment_days ?? 14));
    setLogo(null);
    onCancel();
  };

  const save = async (e?: FormEvent) => {
    e?.preventDefault();
    if (missing.length > 0 || busy) return;
    setBusy(true);
    const values = { ...draft, payment_days: paymentDays };
    let id: string;
    try {
      if (brand) {
        await updateBrand({ ...brand, ...values });
        id = brand.id;
      } else {
        id = await createBrand({ ...values, business_id: businessId, logo_mime: null });
      }
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t, "brand"));
      setBusy(false);
      return;
    }
    // The brand is saved now; a logo that fails must not lead to a second brand on retry.
    try {
      if (logo?.kind === "set") await setBrandLogo(id, logo.path);
      if (logo?.kind === "remove") await removeBrandLogo(id);
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t, "brand"));
    }
    setBusy(false);
    onDone(id);
  };

  const hasLogo = logo?.kind === "set" || (logo === null && logoUrl !== null);
  const fileName = logo?.kind === "set" ? (logo.path.split(/[\\/]/).pop() ?? logo.path) : "";

  return (
    <form id={formId} className="edit-form" onSubmit={save} onKeyDown={saveOnCtrlS}>
      <h2 className="form-title">{brand ? brand.name || t("Brand") : t("New brand")}</h2>

      <section className="form-section">
        <h3>{t("Appearance")}</h3>
        <div className="field-grid">
          <TextField label={t("Name")} value={draft.name} onChange={set("name")} />
          <ColorField label={t("Colour")} value={draft.color} onChange={set("color")} />
          <div className="field wide">
            <span className="field-label">{t("Logo")}</span>
            <div className="logo-row">
              <div className="logo-preview">
                {logo?.kind === "set" ? (
                  <span className="logo-file">
                    <span className="num">{fileName}</span>
                    <span className="field-hint">{t("Taken over when you save")}</span>
                  </span>
                ) : logo === null && logoUrl ? (
                  <img src={logoUrl} alt={t("Logo")} />
                ) : (
                  <span className="field-hint">{t("No logo")}</span>
                )}
              </div>
              <button type="button" className="btn" onClick={pick}>
                <ImagePlus size={16} aria-hidden="true" />
                {t("Choose image")}
              </button>
              {hasLogo && (
                <button
                  type="button"
                  className="btn ghost"
                  onClick={() => setLogo(brand?.logo_mime ? { kind: "remove" } : null)}
                >
                  {t("Remove")}
                </button>
              )}
            </div>
            <span className="field-hint">{t("PNG or JPEG, shown at the top of the invoice.")}</span>
          </div>
          <SelectField
            label={t("Template")}
            value={draft.pdf_template}
            onChange={set("pdf_template")}
            options={[
              { value: "classic", label: t("Classic") },
              { value: "modern", label: t("Modern") },
              { value: "minimal", label: t("Minimal") },
            ]}
          />
          <TextField
            label={t("Invoice prefix")}
            value={draft.invoice_prefix}
            onChange={set("invoice_prefix")}
            hint={t("Comes before the number, as in {example}.", {
              example: `${draft.invoice_prefix || "RE"}-${new Date().getFullYear()}-0001`,
            })}
            mono
          />
        </div>
      </section>

      <section className="form-section">
        <h3>{t("Contact")}</h3>
        <div className="field-grid">
          <TextField label={t("Phone")} type="tel" value={draft.phone} onChange={set("phone")} />
          <TextField label={t("Email")} type="email" value={draft.email} onChange={set("email")} />
          <TextField label={t("Website")} value={draft.website} onChange={set("website")} wide />
        </div>
      </section>

      <section className="form-section">
        <h3>{t("Bank account")}</h3>
        <div className="field-grid">
          <TextField label={t("Bank")} value={draft.bank_name} onChange={set("bank_name")} wide />
          <TextField label={t("IBAN")} value={draft.iban} onChange={set("iban")} mono />
          <TextField label={t("BIC")} value={draft.bic} onChange={set("bic")} mono />
        </div>
      </section>

      <section className="form-section">
        <h3>{t("Invoice texts")}</h3>
        <div className="field-grid">
          <TextField
            label={t("Payment term")}
            value={days}
            onChange={setDays}
            unit={t("days")}
            inputMode="numeric"
            invalid={Number.isNaN(paymentDays) || paymentDays > 365}
            mono
          />
          <SelectField<PriceMode>
            label={t("Prices")}
            value={draft.price_mode}
            onChange={set("price_mode")}
            options={[
              { value: "net", label: t("Net, tax is added") },
              { value: "gross", label: t("Gross, tax is included") },
            ]}
          />
          <TextField
            label={t("Opening text")}
            value={draft.intro_text}
            onChange={set("intro_text")}
            hint={t("Stands above the items.")}
            multiline
          />
          <TextField
            label={t("Closing text")}
            value={draft.outro_text}
            onChange={set("outro_text")}
            hint={t("Stands below the total.")}
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
