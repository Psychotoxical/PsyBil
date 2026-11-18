import { useTranslation } from "react-i18next";
import { Dialog } from "./Dialog";

type Props = {
  title: string;
  text: string;
  confirmLabel: string;
  /** The way back; names what is kept when "Cancel" would be unclear. */
  cancelLabel?: string;
  /** The action removes something for good. */
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
};

/** A question before a step that cannot be taken back. The safe button gets the focus. */
export function ConfirmDialog({
  title,
  text,
  confirmLabel,
  cancelLabel,
  danger,
  onConfirm,
  onCancel,
}: Props) {
  const { t } = useTranslation();
  return (
    <Dialog
      title={title}
      onClose={onCancel}
      footer={
        <>
          <button type="button" className="btn ghost" onClick={onCancel}>
            {cancelLabel ?? t("Cancel")}
          </button>
          <button
            type="button"
            className={danger ? "btn danger" : "btn primary"}
            onClick={onConfirm}
          >
            {confirmLabel}
          </button>
        </>
      }
    >
      <p className="dialog-text">{text}</p>
    </Dialog>
  );
}
