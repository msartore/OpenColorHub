import { defineConfig } from "vite";

// Tauri expects a fixed port and does not want vite to clear the terminal.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    // Tauri uses WebView2 on Windows and WebKitGTK on Linux.
    target: ["chrome105", "safari13"],
    minify: process.env.TAURI_ENV_DEBUG ? false : "esbuild",
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
  },
});
