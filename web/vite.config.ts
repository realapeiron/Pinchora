import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// `npm run dev` serves the app on :5173 and sends API, login and font requests to the Rust server
// (`cargo run -p pinchora -- web`), so cookies and the OAuth callback share one origin. Set
// PINCHORA_WEB__PUBLIC_URL=http://localhost:5173 on the Rust side while doing this.
const backend = process.env.PINCHORA_WEB_BACKEND ?? "http://localhost:8090";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": backend,
      "/auth": backend,
      "/assets/fonts": backend,
    },
  },
  build: {
    outDir: "dist",
    assetsDir: "assets",
    // Fonts come from the Rust server at runtime (the same files the cards use).
    assetsInlineLimit: 0,
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
