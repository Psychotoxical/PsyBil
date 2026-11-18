import { useEffect } from "react";
import { create } from "zustand";

export type Page =
  | "overview"
  | "invoices"
  | "customers"
  | "catalog"
  | "senders"
  | "settings"
  | "import"
  | "help";

export type Route =
  | { page: Page }
  /** A new invoice (`id` empty) or a draft being written. */
  | { page: "assistant"; id?: string }
  /** An issued invoice; `fresh` right after issuing it. */
  | { page: "issued"; id: string; fresh?: boolean };

/** A button in a message, e.g. "Show in folder". */
export type ToastAction = { label: string; run: () => void };

export type Toast = { id: number; kind: "info" | "error"; text: string; action?: ToastAction };

type State = {
  route: Route;
  toasts: Toast[];
  /** Counts saved master data, so that other parts of the window load it again. */
  dataVersion: number;
  bump: () => void;
  /** The sender the overview shows; `null` for all. */
  overviewBusiness: string | null;
  setOverviewBusiness: (id: string | null) => void;
  /** The business of the draft or invoice that is open, for the menu's foot. */
  senderId: string | null;
  setSender: (id: string | null) => void;
  /** A form on the page has changes that are not saved yet. */
  unsaved: boolean;
  setUnsaved: (unsaved: boolean) => void;
  /** Where the person wanted to go while a form was unsaved; the window asks first. */
  pendingRoute: Route | null;
  navigate: (route: Route) => void;
  /** Go where the person wanted, dropping the changes. */
  leave: () => void;
  /** Stay on the form. */
  stay: () => void;
  /** Info messages fade after a while; errors stay until closed. */
  toast: (kind: Toast["kind"], text: string, action?: ToastAction) => void;
  dismiss: (id: number) => void;
};

let next = 1;

const same = (a: Route, b: Route) => JSON.stringify(a) === JSON.stringify(b);

export const useUi = create<State>((set, get) => ({
  route: { page: "overview" },
  toasts: [],
  dataVersion: 0,
  bump: () => set({ dataVersion: get().dataVersion + 1 }),
  overviewBusiness: null,
  setOverviewBusiness: (overviewBusiness) => set({ overviewBusiness }),
  senderId: null,
  setSender: (senderId) => set({ senderId }),
  unsaved: false,
  setUnsaved: (unsaved) => set({ unsaved }),
  pendingRoute: null,
  navigate: (route) => {
    if (same(route, get().route)) return;
    if (get().unsaved) set({ pendingRoute: route });
    else set({ route });
  },
  leave: () => {
    const to = get().pendingRoute;
    set({ unsaved: false, pendingRoute: null, ...(to ? { route: to } : {}) });
  },
  stay: () => set({ pendingRoute: null }),
  toast: (kind, text, action) => {
    // The same message again does not pile up.
    if (get().toasts.some((t) => t.kind === kind && t.text === text)) return;
    const id = next++;
    set({ toasts: [...get().toasts, { id, kind, text, action }] });
    // A message with a button stays a little longer, so there is time to use it.
    if (kind === "info") setTimeout(() => get().dismiss(id), action ? 12000 : 6000);
  },
  dismiss: (id) => set({ toasts: get().toasts.filter((t) => t.id !== id) }),
}));

/** Tells the window that the page's form has unsaved changes, so leaving it asks first. */
export function useUnsavedGuard(dirty: boolean): void {
  useEffect(() => {
    useUi.getState().setUnsaved(dirty);
  }, [dirty]);
  useEffect(() => () => useUi.getState().setUnsaved(false), []);
}
