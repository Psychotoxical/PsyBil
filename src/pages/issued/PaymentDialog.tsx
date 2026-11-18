import { type FormEvent, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { PaymentMethod } from "../../ipc/bindings/PaymentMethod";
import { messageFor, toCmdError } from "../../ipc/errors";
import { addPayment } from "../../ipc/payments";
import { Dialog } from "../../shell/Dialog";
import { useUi } from "../../store/ui";
import { AmountField } from "../../ui/AmountField";
import { SelectField } from "../../ui/SelectField";
import { TextField } from "../../ui/TextField";
import { today } from "../assistant/shared";

/** Cents from the core as a decimal string for the field: 1975 → "19.75". */
function centsToDecimal(cents: number): string {
  if (cents === 0) return "";
  const sign = cents < 0 ? "-" : "";
  const abs = Math.abs(Math.trunc(cents));
  return `${sign}${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, "0")}`;
}

type Props = {
  invoiceId: string;
  /** Still to pay in cents, from `invoice_status`; negative when overpaid. */
  open: number;
  currency: string;
  onDone: () => void;
  onClose: () => void;
};

/** Records a payment; a negative amount is a refund. */
export function PaymentDialog({ invoiceId, open, currency, onDone, onClose }: Props) {
  const { t } = useTranslation();
  const toast = useUi((s) => s.toast);
  const formId = useId();
  const [amount, setAmount] = useState(() => centsToDecimal(open));
  const [valid, setValid] = useState(true);
  const [date, setDate] = useState(today());
  const [method, setMethod] = useState<PaymentMethod>("transfer");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);

  const missing = [
    !valid && t("Amount is not valid"),
    valid && (amount === "" || Number(amount) === 0) && t("Amount is missing"),
    !date && t("Date is missing"),
  ].filter((m): m is string => Boolean(m));

  const save = async (e?: FormEvent) => {
    e?.preventDefault();
    if (missing.length > 0 || busy) return;
    setBusy(true);
    try {
      await addPayment(invoiceId, amount, date, method, note.trim());
      onDone();
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      title={t("Record payment")}
      onClose={onClose}
      footer={
        <>
          {missing.length > 0 && (
            <p className="form-missing" id={`${formId}-missing`}>
              {missing.map((m) => (
                <span key={m}>{m}</span>
              ))}
            </p>
          )}
          <button type="button" className="btn ghost" onClick={onClose}>
            {t("Cancel")}
          </button>
          <button
            type="submit"
            form={formId}
            className="btn primary"
            disabled={missing.length > 0 || busy}
            aria-describedby={missing.length > 0 ? `${formId}-missing` : undefined}
          >
            {t("Record payment")}
          </button>
        </>
      }
    >
      <form id={formId} className="field-grid" onSubmit={save}>
        <AmountField
          label={t("Amount")}
          value={amount}
          currency={currency}
          maxFraction={2}
          hint={t("Negative amounts are refunds.")}
          onChange={(v, ok) => {
            setAmount(v);
            setValid(ok);
          }}
        />
        <TextField label={t("Date")} type="date" value={date} onChange={setDate} />
        <SelectField<PaymentMethod>
          label={t("Method")}
          value={method}
          onChange={setMethod}
          options={[
            { value: "transfer", label: t("Bank transfer") },
            { value: "cash", label: t("Cash") },
            { value: "pay_pal", label: "PayPal" },
            { value: "other", label: t("Other") },
          ]}
        />
        <TextField label={t("Note")} value={note} onChange={setNote} />
      </form>
    </Dialog>
  );
}
