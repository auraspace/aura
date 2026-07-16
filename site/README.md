# Aura site

Marketing homepage + static docs for Aura RFCs (`docs/rfc/`). Built with Vite + React + Tailwind CSS v4; prerenders HTML for GitHub Pages.

## Commands

```bash
# from repo root
pnpm site:dev      # dev server
pnpm site:test     # unit tests
pnpm site:build    # production + prerender → site/dist
pnpm site:preview  # preview dist

# or from site/
pnpm dev
pnpm test
pnpm build
```

## Source layout

Feature-first folders so `/docs`, landing, etc. can land beside `/rfc`:

```text
src/
  app.tsx                 # top-level routes
  components/layout/      # shared chrome (header, theme)
  pages/
    home-page.tsx         # / marketing landing
    not-found-page.tsx
    rfc/                  # /rfc feature
      catalog-page.tsx
      detail-page.tsx
      graph-page.tsx
      components/         # RFC-only UI
      index.ts
  lib/rfc/                # parse, search, graph, types
  styles/
```

Scaffold a new section the same way: `pages/<name>/`, optional `pages/<name>/components/`, `lib/<name>/`, then mount under `<Route path="…">` in `app.tsx`.

## GitHub Pages

1. Repo **Settings → Pages → Source: GitHub Actions**
2. Workflow: `.github/workflows/deploy-site.yml`
3. Base path is `/<repo-name>/` (set via `VITE_BASE` in CI)

Public URLs (repo `aura`):

| Path | Page |
| ---- | ---- |
| https://auraspace.github.io/aura/ | Marketing homepage |
| https://auraspace.github.io/aura/rfc | RFC catalog |
| https://auraspace.github.io/aura/rfc/000 | RFC-000 detail |
| https://auraspace.github.io/aura/rfc/graph | Dependency graph |

Legacy `/graph` redirects to `/rfc/graph`.

## Content

RFC markdown stays in `../docs/rfc/RFC-*.md` (read-only). Rebuild the site after editing RFCs.
