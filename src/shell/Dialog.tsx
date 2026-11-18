import { X } from "lucide-react";
import { type ReactNode, useEffect, useId, useRef } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

type Props = {
  title: string;
  onClose: () => void;
  children: ReactNode;
  /** Buttons at the foot of the dialog. */
  footer?: ReactNode;
  wide?: boolean;
};

/** Open dialogs; while there is one, the window behind it is inert. */
let openCount = 0;

/** A modal dialog: focus stays inside, Esc closes, focus returns to where it was. */
export function Dialog({ title, onClose, children, footer, wide }: Props) {
  const { t } = useTranslation();
  const titleId = useId();
  const box = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;

  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    // The first field of the body, else the first footer button (the safe one), else the box.
    const body = box.current?.querySelector<HTMLElement>(".dialog-body");
    const foot = box.current?.querySelector<HTMLElement>(".dialog-footer");
    const first =
      body?.querySelector<HTMLElement>(FOCUSABLE) ??
      foot?.querySelector<HTMLElement>(FOCUSABLE) ??
      box.current;
    first?.focus();
    const root = document.getElementById("root");
    openCount += 1;
    if (root) root.inert = true;
    return () => {
      openCount -= 1;
      if (root && openCount === 0) root.inert = false;
      before?.focus();
    };
  }, []);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close.current();
      return;
    }
    if (e.key !== "Tab" || !box.current) return;
    const items = [...box.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
    const first = items[0];
    const last = items[items.length - 1];
    if (!first || !last) {
      e.preventDefault();
      return;
    }
    const active = document.activeElement as HTMLElement | null;
    const inside = active ? items.includes(active) : false;
    if (e.shiftKey && (active === first || !inside)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (active === last || !inside)) {
      e.preventDefault();
      first.focus();
    }
  };

  return createPortal(
    // biome-ignore lint/a11y/noStaticElementInteractions: the backdrop only keeps focus in place
    <div
      className="dialog-backdrop"
      // A click beside the dialog keeps the focus where it is.
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) e.preventDefault();
      }}
    >
      <div
        ref={box}
        className={wide ? "dialog wide" : "dialog"}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        onKeyDown={onKeyDown}
      >
        <div className="dialog-body">
          <h2 id={titleId} className="dialog-title">
            {title}
          </h2>
          {children}
        </div>
        {footer && <div className="dialog-footer">{footer}</div>}
        <button
          type="button"
          className="icon-btn dialog-close"
          aria-label={t("Close")}
          onClick={() => close.current()}
        >
          <X size={18} aria-hidden="true" />
        </button>
      </div>
    </div>,
    document.body,
  );
}
