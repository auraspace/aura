import { useEffect } from "react";
import { useLocation } from "react-router-dom";
import { getAllGuideMeta } from "@/lib/docs";
import { pageMetaForRoute } from "@/lib/page-meta";
import { getAllMeta } from "@/lib/rfc/load-rfcs";
import { absoluteUrl } from "@/lib/site";

function upsertMeta(
  attr: "name" | "property",
  key: string,
  content: string,
): void {
  const selector = `meta[${attr}="${key}"]`;
  let el = document.head.querySelector(selector);
  if (!el) {
    el = document.createElement("meta");
    el.setAttribute(attr, key);
    document.head.appendChild(el);
  }
  el.setAttribute("content", content);
}

function upsertCanonical(href: string): void {
  let el = document.head.querySelector('link[rel="canonical"]');
  if (!el) {
    el = document.createElement("link");
    el.setAttribute("rel", "canonical");
    document.head.appendChild(el);
  }
  el.setAttribute("href", href);
}

/**
 * Keeps document title, description, and canonical/OG tags in sync
 * during client-side navigations (prerender already bakes these into HTML).
 */
export function DocumentMeta() {
  const { pathname } = useLocation();

  useEffect(() => {
    const guides = getAllGuideMeta();
    const rfcs = getAllMeta();
    const meta = pageMetaForRoute(pathname, { guides, rfcs });
    const url = absoluteUrl(meta.path, import.meta.env.BASE_URL || "/");

    document.title = meta.title;
    upsertMeta("name", "description", meta.description);
    upsertCanonical(url);
    upsertMeta("property", "og:type", "website");
    upsertMeta("property", "og:site_name", "Aura");
    upsertMeta("property", "og:title", meta.title);
    upsertMeta("property", "og:description", meta.description);
    upsertMeta("property", "og:url", url);
    upsertMeta("name", "twitter:card", "summary");
    upsertMeta("name", "twitter:title", meta.title);
    upsertMeta("name", "twitter:description", meta.description);
  }, [pathname]);

  return null;
}
