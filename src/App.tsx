import "./shell/shell.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { setLanguage } from "./i18n";
import { Assistant } from "./pages/assistant/Assistant";
import { Catalog } from "./pages/catalog/Catalog";
import { Customers } from "./pages/customers/Customers";
import { InvoiceList } from "./pages/invoices/InvoiceList";
import { IssuedView } from "./pages/issued/IssuedView";
import { Senders } from "./pages/senders/Senders";
import { Settings } from "./pages/settings/Settings";
import { ConfirmDialog } from "./shell/ConfirmDialog";
import { Sidebar } from "./shell/Sidebar";
import { Toasts } from "./shell/Toasts";
import { flushDraft } from "./store/draft";
import { useSettings } from "./store/settings";
import { type Route, useUi } from "./store/ui";
import { applyTheme, followSystem } from "./theme";

/** The page for a route. */
function Page({ route }: { route: Route }) {
  switch (route.page) {
    case "invoices":
      return <InvoiceList />;
    case "customers":
      return <Customers />;
    case "catalog":
      return <Catalog />;
    case "senders":
      return <Senders />;
    case "settings":
      return <Settings />;
    case "assistant":
      return <Assistant id={route.id} />;
    case "issued":
      return <IssuedView key={route.id} id={route.id} fresh={route.fresh} />;
  }
}

/** The window: menu on the left, the current page beside it. */
export function App() {
  const { t } = useTranslation();
  const route = useUi((s) => s.route);
  const navigate = useUi((s) => s.navigate);
  const pendingRoute = useUi((s) => s.pendingRoute);
  const [closing, setClosing] = useState(false);
  const theme = useSettings((s) => s.settings?.theme ?? "system");
  const language = useSettings((s) => s.settings?.language);
  const load = useSettings((s) => s.load);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    applyTheme(theme);
    return followSystem(() => useSettings.getState().settings?.theme ?? "system");
  }, [theme]);

  useEffect(() => {
    if (language === "de" || language === "en") setLanguage(language);
  }, [language]);

  // Closing the window writes a waiting draft and settings first, and asks about an unsaved form.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let gone = false;
    try {
      getCurrentWindow()
        .onCloseRequested(async (event) => {
          await flushDraft();
          await useSettings.getState().flush();
          if (useUi.getState().unsaved) {
            event.preventDefault();
            setClosing(true);
          }
        })
        .then((stop) => {
          if (gone) stop();
          else unlisten = stop;
        })
        .catch(() => {});
    } catch {
      // Outside the app there is no window to watch.
    }
    return () => {
      gone = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "n") {
        e.preventDefault();
        navigate({ page: "assistant" });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navigate]);

  return (
    <div className="app">
      <Sidebar />
      <main className={route.page === "assistant" ? "page bleed" : "page"}>
        <Page route={route} />
      </main>
      <Toasts />
      {(pendingRoute || closing) && (
        <ConfirmDialog
          title={t("Discard changes?")}
          text={t("The changes in this form are not saved yet.")}
          confirmLabel={t("Discard")}
          cancelLabel={t("Keep editing")}
          onCancel={() => {
            setClosing(false);
            useUi.getState().stay();
          }}
          onConfirm={() => {
            if (closing) {
              useUi.getState().setUnsaved(false);
              void getCurrentWindow().destroy();
              return;
            }
            useUi.getState().leave();
          }}
        />
      )}
    </div>
  );
}
