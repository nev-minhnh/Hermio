import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { readFileSync } from "node:fs";

// The version shown in the app comes from package.json (kept in step with Cargo.toml / tauri.conf.json).
const pkg = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8"));

export default defineConfig({
  plugins: [svelte()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
    __BUILD_DATE__: JSON.stringify(new Date().toISOString().slice(0, 10)),
  },
  clearScreen: false,
  server: { port: 1430, strictPort: true },
  build: { target: "es2022", minify: true, cssMinify: true, sourcemap: false },
});
