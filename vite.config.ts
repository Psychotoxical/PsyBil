/// <reference types="vitest/config" />
import { readFileSync } from "node:fs";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const pkg = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8"));

// Tauri expects a fixed port and its own console output.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  define: { __APP_VERSION__: JSON.stringify(pkg.version) },
  // Rust builds write into target/ and src-tauri/; watching them only costs locks.
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/target/**", "**/src-tauri/**"] },
  },
  build: { target: "es2022", outDir: "dist" },
  test: { environment: "jsdom", setupFiles: ["src/test/setup.ts"] },
});
