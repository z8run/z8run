import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: "http://localhost:7700",
        changeOrigin: true,
      },
      "/auth": {
        target: "http://localhost:7700",
        changeOrigin: true,
      },
      "/hook": {
        target: "http://localhost:7700",
        changeOrigin: true,
      },
      "/ws": {
        target: "http://localhost:7700",
        ws: true,
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: "dist",
    sourcemap: true,
  },
});
