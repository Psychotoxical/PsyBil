import { getCurrentWindow } from "@tauri-apps/api/window";
import type { ThemeChoice } from "./ipc/bindings/ThemeChoice";

const dark = () =>
  typeof window.matchMedia === "function" &&
  window.matchMedia("(prefers-color-scheme: dark)").matches;

/** Sets Mocha or Latte on the page; "system" follows the operating system. The window frame
 *  (title bar) follows too, so a dark page does not sit under a light bar. */
export function applyTheme(choice: ThemeChoice): void {
  const isDark = choice === "dark" || (choice === "system" && dark());
  document.documentElement.dataset.theme = isDark ? "mocha" : "latte";
  try {
    void getCurrentWindow()
      .setTheme(choice === "system" ? null : choice)
      .catch(() => {});
  } catch {
    // Outside the app there is no window frame.
  }
}

/** Re-applies "system" when the operating system switches. */
export function followSystem(getChoice: () => ThemeChoice): () => void {
  if (typeof window.matchMedia !== "function") return () => {};
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  const onChange = () => {
    if (getChoice() === "system") applyTheme("system");
  };
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}
