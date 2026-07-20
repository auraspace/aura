import { describe, expect, it } from "vitest";
import {
  applyPageMetaToHtml,
  pageMetaForRoute,
  seoHeadHtml,
} from "./page-meta";
import { DEFAULT_TITLE, SITE_ORIGIN, escapeHtmlAttr } from "./site";

const ctx = {
  guides: [
    {
      slug: "getting-started",
      title: "Getting started",
      summary: "Install Aura and run hello.",
    },
  ],
  rfcs: [{ id: "000", title: "Vision & Design Principles" }],
};

describe("pageMetaForRoute", () => {
  it("resolves static hubs", () => {
    expect(pageMetaForRoute("/").title).toBe(DEFAULT_TITLE);
    expect(pageMetaForRoute("/docs").title).toContain("Documentation");
    expect(pageMetaForRoute("/rfc").title).toContain("RFC catalog");
    expect(pageMetaForRoute("/rfc/graph").path).toBe("/rfc/graph");
  });

  it("resolves guide and RFC pages from context", () => {
    const guide = pageMetaForRoute("/docs/getting-started", ctx);
    expect(guide.title).toContain("Getting started");
    expect(guide.description).toContain("Install Aura");

    const rfc = pageMetaForRoute("/rfc/0", ctx);
    expect(rfc.path).toBe("/rfc/000");
    expect(rfc.title).toContain("RFC-000");
    expect(rfc.title).toContain("Vision");
  });

  it("falls back for unknown routes", () => {
    expect(pageMetaForRoute("/missing", ctx).title).toContain("Not found");
  });
});

describe("seoHeadHtml / applyPageMetaToHtml", () => {
  it("emits canonical and og:url for the production origin", () => {
    const meta = pageMetaForRoute("/docs", ctx);
    const head = seoHeadHtml(meta);
    expect(head).toContain(`rel="canonical" href="${SITE_ORIGIN}/docs"`);
    expect(head).toContain(`property="og:url" content="${SITE_ORIGIN}/docs"`);
    expect(head).toContain('property="og:site_name" content="Aura"');
  });

  it("rewrites title, description, and injects SEO block", () => {
    const shell = `<!doctype html><html><head>
    <meta name="description" content="old" />
    <title>Old</title>
  </head><body></body></html>`;
    const meta = pageMetaForRoute("/rfc/000", ctx);
    const out = applyPageMetaToHtml(shell, meta);
    expect(out).toContain(`<title>${escapeHtmlAttr(meta.title)}</title>`);
    expect(out).toContain(`content="${escapeHtmlAttr(meta.description)}"`);
    expect(out).toContain(`href="${SITE_ORIGIN}/rfc/000"`);
    expect(out).toContain("<!--seo:start-->");
    // Idempotent
    const twice = applyPageMetaToHtml(out, meta);
    expect(twice.match(/rel="canonical"/g)?.length).toBe(1);
  });
});
