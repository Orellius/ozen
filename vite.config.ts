import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The pill window is plain HTML in public/ on purpose: it has no imports, it must stay a
// standalone document Tauri can load by URL, and bundling it would gain nothing.
export default defineConfig({
  plugins: [react()],
  // Tauri owns the console output; Vite clearing it hides Rust panics mid-session.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "safari15",
    sourcemap: true,
  },
});
