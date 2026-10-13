import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  // Keep Rust errors visible in `tauri dev`.
  clearScreen: false,
  server: {
    // Tauri expects a fixed port.
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
