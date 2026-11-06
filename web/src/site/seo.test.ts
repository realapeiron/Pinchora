import { describe, expect, it } from "vitest";

import { PUBLIC_ROUTES } from "./routes";
import { SEO_END, SEO_START, escapeHtml, seoHead, sitemap } from "./seo";

describe("seoHead", () => {
  const [front, commands] = PUBLIC_ROUTES;
  if (!front || !commands) throw new Error("routes");

  it("writes a title, a description, a canonical link and the preview tags", () => {
    const html = seoHead(commands);
    expect(html.startsWith(SEO_START)).toBe(true);
    expect(html.endsWith(SEO_END)).toBe(true);
    expect(html).toContain(`<title>${commands.title}</title>`);
    expect(html).toContain(`<meta name="description" content="${commands.description}" />`);
    expect(html).toContain('<link rel="canonical" href="__ORIGIN__/commands" />');
    for (const tag of ["og:title", "og:description", "og:url", "og:image", "og:image:alt", "og:site_name"]) {
      expect(html, tag).toContain(`property="${tag}"`);
    }
    for (const tag of ["twitter:card", "twitter:title", "twitter:description", "twitter:image"]) {
      expect(html, tag).toContain(`name="${tag}"`);
    }
    expect(html).toContain('content="summary_large_image"');
    expect(html).toContain('property="og:image" content="__ORIGIN__/og.png"');
    expect(html).toContain('property="og:image:width" content="1200"');
  });

  it("uses the site's address with a trailing slash for the front page", () => {
    expect(seoHead(front)).toContain('<link rel="canonical" href="__ORIGIN__/" />');
  });

  it("describes the app as structured data on the front page only", () => {
    expect(seoHead(front)).toContain("application/ld+json");
    expect(seoHead(commands)).not.toContain("application/ld+json");
  });

  it("escapes what it writes into attributes", () => {
    expect(escapeHtml(`a "b" <c> & d`)).toBe("a &quot;b&quot; &lt;c&gt; &amp; d");
    const html = seoHead({ ...commands, title: 'Say "hi" <now>', description: "A & B" });
    expect(html).toContain("<title>Say &quot;hi&quot; &lt;now&gt;</title>");
    expect(html).toContain('content="A &amp; B"');
  });

  it("makes every page's head different and every title its own", () => {
    const heads = PUBLIC_ROUTES.map((r) => seoHead(r));
    expect(new Set(heads).size).toBe(heads.length);
  });
});

describe("sitemap", () => {
  it("lists every public page with the address placeholder and nothing else", () => {
    const xml = sitemap(PUBLIC_ROUTES);
    expect(xml.startsWith('<?xml version="1.0" encoding="UTF-8"?>')).toBe(true);
    for (const route of PUBLIC_ROUTES) {
      expect(xml).toContain(`<loc>__ORIGIN__${route.path === "/" ? "/" : route.path}</loc>`);
    }
    expect((xml.match(/<url>/g) ?? []).length).toBe(PUBLIC_ROUTES.length);
    expect(xml).not.toContain("/servers");
    expect(xml).not.toContain("/login");
  });
});
