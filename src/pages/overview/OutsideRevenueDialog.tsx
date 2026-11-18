import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { centsToDecimal } from "../../format";
import type { SmallBusinessMeter } from "../../ipc/bindings/SmallBusinessMeter";
import { messageFor, toCmdError } from "../../ipc/errors";
import { outsideRevenue, setOutsideRevenue } from "../../ipc/overview";
import { Dialog } from "../../shell/Dialog";
import { useUi } from "../../store/ui";
import { AmountField } from "../../ui/AmountField";
import { Choice } from "../../ui/Choice";
import { TextField } from "../../ui/TextField";

type Props = {
  meter: SmallBusinessMeter;
  currency: string;
  onClose: () => void;
  onSaved: () => void;
};

/** The revenue of one year that did not run through PsyBil, as one sum. */
export function OutsideRevenueDialog({ meter, currency, onClose, onSaved }: Props) {
  const { t } = useTranslation();
  const toast = useUi((s) => s.toast);
  const [year, setYear] = useState(meter.year);
  const [amount, setAmount] = useState("");
  const [valid, setValid] = useState(true);
  const [note, setNote] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let live = true;
    outsideRevenue(meter.business_id, year)
      .then((found) => {
        if (!live) return;
        setAmount(found ? centsToDecimal(found.amount) : "");
        setNote(found?.note ?? "");
        setValid(true);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [meter.business_id, year]);

  const save = async () => {
    setSaving(true);
    try {
      await setOutsideRevenue(meter.business_id, year, amount === "" ? "0" : amount, note);
      onSaved();
    } catch (error) {
      toast("error", messageFor(toCmdError(error), t));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog
      title={t("Revenue outside PsyBil")}
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            {t("Cancel")}
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={!valid || saving}
            onClick={() => void save()}
          >
            {t("Save")}
          </button>
        </>
      }
    >
      <p className="dialog-text">
        {t(
          "Platform payouts, cash at the merch table and the like, as one sum per year for {name}.",
          { name: meter.business_name },
        )}
      </p>
      <Choice
        label={t("Year")}
        value={year}
        options={[meter.year, meter.year - 1].map((y) => ({ value: y, label: String(y) }))}
        onChange={setYear}
      />
      <AmountField
        label={t("Amount")}
        value={amount}
        currency={currency}
        maxFraction={2}
        onChange={(value, ok) => {
          setAmount(value);
          setValid(ok);
        }}
      />
      <TextField label={t("Note")} value={note} onChange={setNote} />
    </Dialog>
  );
}
