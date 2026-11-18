import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync("src/styles/tokens.css", "utf8");

/** The custom properties of the block for `theme`. */
function tokens(theme: "mocha" | "latte"): Record<string, string> {
  const block = css.split(`[data-theme="${theme}"]`)[1]?.split("}")[0] ?? "";
  return Object.fromEntries(
    [...block.matchAll(/(--[\w-]+):\s*(#[0-9a-fA-F]{6})/g)].map((m) => [m[1], m[2]]),
  );
}

function luminance(hex: string): number {
  const channel = (i: number) => {
    const c = Number.parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

const PAIRS: [string, string][] = [
  ["--text", "--bg-app"],
  ["--text", "--bg-chrome"],
  ["--text", "--bg-field"],
  ["--text", "--bg-button"],
  ["--text-secondary", "--bg-app"],
  ["--text-secondary", "--bg-chrome"],
  ["--error", "--bg-app"],
  ["--ok-text", "--ok-bg"],
  ["--bad-text", "--bad-bg"],
  ["--accent-text", "--accent"],
  // Units and placeholders inside fields; the delete button and its hover.
  ["--text-secondary", "--bg-field"],
  ["--accent-text", "--error"],
  ["--bad-bg", "--bad-text"],
  // Status badges.
  ["--text", "--warning-bg"],
  ["--text", "--row-alt"],
  ["--text-secondary", "--row-alt"],
];

describe("theme", () => {
  it("theme_text_pairs_meet_wcag_aa", () => {
    for (const theme of ["mocha", "latte"] as const) {
      const t = tokens(theme);
      for (const [fg, bg] of PAIRS) {
        expect(t[fg], `${theme} ${fg}`).toBeDefined();
        expect(t[bg], `${theme} ${bg}`).toBeDefined();
        const ratio = contrast(t[fg] as string, t[bg] as string);
        expect(ratio, `${theme}: ${fg} on ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("accent_is_mauve_in_both_themes", () => {
    expect(tokens("mocha")["--accent"]).toBe("#cba6f7");
    expect(tokens("latte")["--accent"]).toBe("#8839ef");
  });
});
