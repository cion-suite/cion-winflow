import { resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": resolve(import.meta.dirname, "./src"),
    },
  },

  server: {
    port: 1420,
    strictPort: true,
    // Dev-фронтенд ходит в axum через прокси: origin один, CORS не нужен
    proxy: {
      "/api": "http://127.0.0.1:3000",
    },
  },
}));
