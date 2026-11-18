import {
  Building2,
  FileText,
  LayoutDashboard,
  LifeBuoy,
  type LucideIcon,
  Package,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Settings,
  Users,
} from "lucide-react";
import { type CSSProperties, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import wordmarkUrl from "../assets/wordmark-psybil.svg";
import type { Business } from "../ipc/bindings/Business";
import { listBusinesses } from "../ipc/masterdata";
import { currentSettings, useSettings } from "../store/settings";
import { type Page, useUi } from "../store/ui";
import { useReadOnly } from "../store/workspace";

type Item = { page: Page; label: string; icon: LucideIcon };

// The wordmark is a mask painted in the accent colour. The URL comes from the
// bundler, because a relative url() inside a CSS mask is not rewritten.
const WORDMARK_STYLE = { "--wordmark": `url("${wordmarkUrl}")` } as CSSProperties;

/** The menu on the left: wordmark, pages, "New invoice", the current sender. */
export function Sidebar() {
  const { t } = useTranslation();
  const route = useUi((s) => s.route);
  const navigate = useUi((s) => s.navigate);
  const collapsed = useSettings((s) => s.settings?.sidebar_collapsed ?? false);
  const update = useSettings((s) => s.update);
  const dataVersion = useUi((s) => s.dataVersion);
  const senderId = useUi((s) => s.senderId);
  const [businesses, setBusinesses] = useState<Business[]>([]);
  const readOnly = useReadOnly();
  const reasonId = useId();

  // biome-ignore lint/correctness/useExhaustiveDependencies: dataVersion signals saved data
  useEffect(() => {
    listBusinesses(false)
      .then(setBusinesses)
      .catch(() => setBusinesses([]));
  }, [dataVersion]);

  // The sender of what is open; with one sender always that one; with several and nothing
  // open, how many there are.
  const business =
    businesses.find((b) => b.id === senderId) ??
    (businesses.length === 1 ? businesses[0] : undefined);

  const work: Item[] = [
    { page: "overview", label: t("Overview"), icon: LayoutDashboard },
    { page: "invoices", label: t("Invoices"), icon: FileText },
    { page: "customers", label: t("Customers"), icon: Users },
    { page: "catalog", label: t("Catalogue"), icon: Package },
  ];
  const setup: Item[] = [
    { page: "senders", label: t("Senders"), icon: Building2 },
    { page: "settings", label: t("Settings"), icon: Settings },
    { page: "help", label: t("Help"), icon: LifeBuoy },
  ];
  const current = route.page === "issued" || route.page === "assistant" ? "invoices" : route.page;

  const item = ({ page, label, icon: Icon }: Item) => (
    <li key={page}>
      <button
        type="button"
        className="nav-item"
        data-page={page}
        aria-current={current === page ? "page" : undefined}
        title={collapsed ? label : undefined}
        onClick={() => navigate({ page })}
      >
        <Icon size={18} aria-hidden="true" />
        <span className={collapsed ? "visually-hidden" : "nav-label"}>{label}</span>
      </button>
    </li>
  );

  return (
    <nav className={collapsed ? "sidebar collapsed" : "sidebar"} aria-label={t("Main menu")}>
      <div className="sidebar-top">
        <span className="wordmark" role="img" aria-label="PsyBil" style={WORDMARK_STYLE} />
        <button
          type="button"
          className="icon-btn"
          aria-label={collapsed ? t("Expand menu") : t("Collapse menu")}
          onClick={() => update({ sidebar_collapsed: !currentSettings().sidebar_collapsed })}
        >
          {collapsed ? (
            <PanelLeftOpen size={18} aria-hidden="true" />
          ) : (
            <PanelLeftClose size={18} aria-hidden="true" />
          )}
        </button>
      </div>
      <button
        type="button"
        className="btn primary new-invoice"
        title={readOnly ?? (collapsed ? t("New invoice") : undefined)}
        aria-keyshortcuts="Control+N"
        disabled={readOnly !== null}
        aria-describedby={readOnly ? reasonId : undefined}
        onClick={() => navigate({ page: "assistant" })}
      >
        <Plus size={18} aria-hidden="true" />
        <span className={collapsed ? "visually-hidden" : undefined}>{t("New invoice")}</span>
      </button>
      {readOnly && (
        <span id={reasonId} className="visually-hidden">
          {readOnly}
        </span>
      )}
      <ul className="nav-list">{work.map(item)}</ul>
      <p className={collapsed ? "visually-hidden" : "nav-group"}>{t("Setup")}</p>
      <ul className="nav-list">{setup.map(item)}</ul>
      {!collapsed && business && (
        <div className="sidebar-foot">
          <span className="sender-name">{business.name}</span>
          <span className="sender-note">
            {business.small_business ? t("Small business · § 19") : t("Regular VAT")}
          </span>
        </div>
      )}
      {!collapsed && !business && businesses.length > 1 && (
        <div className="sidebar-foot">
          <span className="sender-name">{t("{n} senders", { n: String(businesses.length) })}</span>
        </div>
      )}
    </nav>
  );
}
