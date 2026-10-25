import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      "/api": { target: process.env.WORKBENCH_API_URL ?? "http://localhost:8000", rewrite: p => p.replace(/^\/api/, "") },
      // The Rust workbench service keeps its /native paths (same origin through this proxy).
      "/native": { target: process.env.WORKBENCH_NATIVE_URL ?? "http://127.0.0.1:8003" },
    },
  },
});
