import { describe, expect, it, vi } from "vitest";
import { applyTheme } from "./theme";

const setTheme = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ setTheme }) }));

describe("theme frame", () => {
  it("window_frame_follows_the_choice", () => {
    applyTheme("dark");
    expect(document.documentElement.dataset.theme).toBe("mocha");
    expect(setTheme).toHaveBeenLastCalledWith("dark");
    applyTheme("light");
    expect(setTheme).toHaveBeenLastCalledWith("light");
    applyTheme("system");
    expect(setTheme).toHaveBeenLastCalledWith(null);
  });
});
