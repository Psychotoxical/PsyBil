import { create } from "zustand";
import type { DeviceSettings } from "../ipc/bindings/DeviceSettings";
import { loadSettings, saveSettings } from "../ipc/settings";

/** Used until the core answers, and outside the app (browser preview, tests). */
export const DEFAULT_SETTINGS: DeviceSettings = {
  device_id: "",
  device_name: "",
  theme: "system",
  language: "de",
  sidebar_collapsed: false,
  preview_open: true,
};

type State = {
  settings: DeviceSettings | null;
  load: () => Promise<void>;
  /** Changes settings at once and saves them shortly after. */
  update: (change: Partial<DeviceSettings>) => void;
  /** Writes a pending save now, e.g. before the window closes. */
  flush: () => Promise<void>;
};

let pending: ReturnType<typeof setTimeout> | undefined;

export const useSettings = create<State>((set, get) => ({
  settings: null,
  load: async () => {
    try {
      const loaded = await loadSettings();
      if (loaded) set({ settings: loaded });
    } catch {
      // Outside the app there is nothing to load.
    }
  },
  update: (change) => {
    const next = { ...(get().settings ?? DEFAULT_SETTINGS), ...change };
    set({ settings: next });
    clearTimeout(pending);
    pending = setTimeout(() => {
      pending = undefined;
      void saveSettings(next).catch(() => {});
    }, 400);
  },
  flush: async () => {
    if (pending === undefined) return;
    clearTimeout(pending);
    pending = undefined;
    const settings = get().settings;
    if (settings) await saveSettings(settings).catch(() => {});
  },
}));

export function currentSettings(): DeviceSettings {
  return useSettings.getState().settings ?? DEFAULT_SETTINGS;
}
