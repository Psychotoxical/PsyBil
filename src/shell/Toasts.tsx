import { CircleAlert, Info, X } from "lucide-react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { useUi } from "../store/ui";

/** Messages: info in a polite live region, errors as alerts that stay until closed. */
export function Toasts() {
  const { t } = useTranslation();
  const toasts = useUi((s) => s.toasts);
  const dismiss = useUi((s) => s.dismiss);
  // Outside the app root, so messages are still read out while a dialog makes the window inert.
  return createPortal(
    <div className="toasts" aria-live="polite">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={`toast ${toast.kind}`}
          role={toast.kind === "error" ? "alert" : "status"}
        >
          {toast.kind === "error" ? (
            <CircleAlert size={18} aria-hidden="true" />
          ) : (
            <Info size={18} aria-hidden="true" />
          )}
          <span>{toast.text}</span>
          <button
            type="button"
            className="icon-btn"
            aria-label={t("Close message")}
            onClick={() => dismiss(toast.id)}
          >
            <X size={16} aria-hidden="true" />
          </button>
        </div>
      ))}
    </div>,
    document.body,
  );
}
