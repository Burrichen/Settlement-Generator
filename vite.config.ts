import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// See https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  root: "app",
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/target/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "chrome120",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  test: {
    environment: "jsdom",
  },
});
