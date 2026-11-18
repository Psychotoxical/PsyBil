import { PanelRightClose, PanelRightOpen } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

type Props = {
  title: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
};

/** A folding panel on the right, like the Psysonic queue. */
export function SidePanel({ title, open, onToggle, children }: Props) {
  const { t } = useTranslation();
  const label = open ? t("Hide {title}", { title }) : t("Show {title}", { title });
  return (
    <aside className={open ? "side-panel" : "side-panel closed"} aria-label={title}>
      <div className="side-panel-head">
        {open && <h2 className="side-panel-title">{title}</h2>}
        <button type="button" className="icon-btn" aria-label={label} onClick={onToggle}>
          {open ? (
            <PanelRightClose size={18} aria-hidden="true" />
          ) : (
            <PanelRightOpen size={18} aria-hidden="true" />
          )}
        </button>
      </div>
      {open && <div className="side-panel-body">{children}</div>}
    </aside>
  );
}
