import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  // Prevent vite from obscuring Rust errors
  clearScreen: false,
  // Preserve symlinks/junctions so Vite doesn't resolve through them.
  // Without this, building from a Windows junction (e.g. workspace-proteus)
  // produces invalid relative paths like "../../../../workspace/.../index.html"
  // which Rollup rejects during asset emission.
  resolve: {
    preserveSymlinks: true,
  },
  server: {
    port: 1420,
    strictPort: true,
  },
  // Env variables starting with TAURI_ are exposed to the frontend
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    // Tauri uses Chromium on Windows and WebKit on macOS/Linux
    target: process.env.TAURI_PLATFORM === "windows" ? "chrome105" : "safari13",
    // Don't minify in debug builds
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
    outDir: "dist",
  },
});
