import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

import react from "@vitejs/plugin-react";
import type { Plugin } from "vite";
import { defineConfig } from "vitest/config";

import { PUBLIC_ROUTES } from "./src/site/routes";
import { SEO_END, SEO_START, seoHead, sitemap } from "./src/site/seo";

// `npm run dev` serves the app on :5173 and sends API, login and font requests to the Rust server
// (`cargo run -p pinchora -- web`), so cookies and the OAuth callback share one origin. Set
// PINCHORA_WEB__PUBLIC_URL=http://localhost:5173 on the Rust side while doing this.
const backend = process.env.PINCHORA_WEB_BACKEND ?? "http://localhost:8090";

/**
 * Writes each public page's own tags into the HTML: the shell carries the front page's title, description and
 * link-preview tags between two markers, and the build copies it to `commands/index.html`, `privacy/index.html`
 * and so on with that page's tags instead (crawlers and link unfurlers do not run scripts). Also writes the
 * sitemap. The server puts the site's real address where the build wrote `__ORIGIN__`.
 */
function publicPages(): Plugin {
  const [front] = PUBLIC_ROUTES;
  return {
    name: "pinchora-public-pages",
    transformIndexHtml: {
      order: "pre",
      handler: (html) => html.replace("<!--seo-->", front ? seoHead(front) : ""),
    },
    writeBundle(options) {
      const dir = options.dir;
      if (!dir) return;
      const shell = readFileSync(join(dir, "index.html"), "utf8");
      const block = new RegExp(`${SEO_START}[\\s\\S]*?${SEO_END}`);
      for (const route of PUBLIC_ROUTES.slice(1)) {
        const file = join(dir, route.path.slice(1), "index.html");
        mkdirSync(dirname(file), { recursive: true });
        writeFileSync(
          file,
          shell.replace(block, () => seoHead(route)),
        );
      }
      writeFileSync(join(dir, "sitemap.xml"), sitemap(PUBLIC_ROUTES));
    },
  };
}

export default defineConfig({
  // Do not load local dotenv files; backend configuration stays with the Rust process.
  envDir: "/nonexistent/pinchora-vite-env",
  plugins: [react(), publicPages()],
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": backend,
      "/auth": backend,
      "/invite": backend,
      "/assets/fonts": backend,
    },
  },
  build: {
    manifest: true,
    outDir: "dist",
    assetsDir: "assets",
    // Fonts come from the Rust server at runtime (the same files the cards use).
    assetsInlineLimit: 0,
  },
  test: {
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    environment: "node",
  },
});
