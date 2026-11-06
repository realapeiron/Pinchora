// The tags that make a page look right in search results and link previews. Written into each page's HTML by
// the build (crawlers and link unfurlers do not run scripts), and kept in step while the app changes pages.

import { OG_IMAGE, ORIGIN, SITE_NAME, type PublicRoute } from "./routes";

export const SEO_START = "<!--seo:start-->";
export const SEO_END = "<!--seo:end-->";

export function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function structuredData(): string {
  const data = {
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    name: SITE_NAME,
    applicationCategory: "GameApplication",
    operatingSystem: "Discord",
    description:
      "A Discord bot with an isometric pixel house, pets, fishing, games, levels, pixel-art cards and a web dashboard.",
    url: `${ORIGIN}/`,
    image: `${ORIGIN}${OG_IMAGE.path}`,
  };
  return `<script type="application/ld+json">${JSON.stringify(data)}</script>`;
}

/** The title, description, canonical link, Open Graph and Twitter tags of `route`, between two markers. */
export function seoHead(route: PublicRoute): string {
  const title = escapeHtml(route.title);
  const description = escapeHtml(route.description);
  const url = `${ORIGIN}${route.path === "/" ? "/" : route.path}`;
  const image = `${ORIGIN}${OG_IMAGE.path}`;
  const alt = escapeHtml(OG_IMAGE.alt);
  const lines = [
    SEO_START,
    `<title>${title}</title>`,
    `<meta name="description" content="${description}" />`,
    `<link rel="canonical" href="${url}" />`,
    `<meta property="og:type" content="website" />`,
    `<meta property="og:site_name" content="${SITE_NAME}" />`,
    `<meta property="og:title" content="${title}" />`,
    `<meta property="og:description" content="${description}" />`,
    `<meta property="og:url" content="${url}" />`,
    `<meta property="og:image" content="${image}" />`,
    `<meta property="og:image:type" content="image/png" />`,
    `<meta property="og:image:width" content="${OG_IMAGE.width}" />`,
    `<meta property="og:image:height" content="${OG_IMAGE.height}" />`,
    `<meta property="og:image:alt" content="${alt}" />`,
    `<meta name="twitter:card" content="summary_large_image" />`,
    `<meta name="twitter:title" content="${title}" />`,
    `<meta name="twitter:description" content="${description}" />`,
    `<meta name="twitter:image" content="${image}" />`,
    `<meta name="twitter:image:alt" content="${alt}" />`,
  ];
  if (route.path === "/") lines.push(structuredData());
  lines.push(SEO_END);
  return lines.join("\n    ");
}

/** `sitemap.xml` for `routes`, with the placeholder address the server replaces. */
export function sitemap(routes: PublicRoute[]): string {
  const urls = routes
    .map(
      (r) =>
        `  <url>\n    <loc>${ORIGIN}${r.path === "/" ? "/" : r.path}</loc>\n    <changefreq>${r.changefreq}</changefreq>\n    <priority>${r.priority.toFixed(1)}</priority>\n  </url>`,
    )
    .join("\n");
  return `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`;
}
