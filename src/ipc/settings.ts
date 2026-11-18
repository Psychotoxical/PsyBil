import type { DeviceSettings } from "./bindings/DeviceSettings";
import { call } from "./errors";

export const loadSettings = () => call<DeviceSettings>("load_settings");
export const saveSettings = (settings: DeviceSettings) => call<void>("save_settings", { settings });
export const frontendReady = () => call<void>("frontend_ready");
