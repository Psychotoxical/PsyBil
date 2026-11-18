import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import de from "../locales/de.json";

function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return name === "bindings" ? [] : sources(path);
    return /\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name) ? [path] : [];
  });
}

describe("i18n", () => {
  it("every_t_literal_has_german_text", () => {
    const root = "src";
    const missing: string[] = [];
    for (const file of sources(root)) {
      const text = readFileSync(file, "utf8");
      for (const m of text.matchAll(/\bt\(\s*"((?:[^"\\]|\\.)+)"/g)) {
        const key = (m[1] as string).replace(/\\"/g, '"');
        if (!(key in de)) missing.push(`${file}: ${key}`);
      }
    }
    expect(missing).toEqual([]);
  });
});
