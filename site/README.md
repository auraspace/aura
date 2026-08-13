# Aura site

Marketing homepage, user guide (`docs/guide/` → `/docs`), and RFC catalog (`docs/rfc/` → `/rfc`). Built with Vite + React + Tailwind CSS v4; prerenders HTML for **GitHub Pages**.

## Commands

`site/` is a **pnpm workspace package** (`aura-site`) in the monorepo. Install once from the **repo root**:

```bash
pnpm install
```

Then:

```bash
# from repo root (recommended)
pnpm site:dev      # dev server
pnpm site:test     # unit tests
pnpm site:build    # production + prerender → site/dist
pnpm site:preview  # preview dist

# or filter / run inside the package
pnpm --filter aura-site dev
pnpm --filter aura-site test
pnpm --filter aura-site build
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
    docs/                 # /docs user guide
      hub-page.tsx
      detail-page.tsx
      components/
      index.ts
    rfc/                  # /rfc feature
      catalog-page.tsx
      detail-page.tsx
      graph-page.tsx
      components/         # RFC-only UI
      index.ts
  lib/docs/               # guide parse + nav
  lib/rfc/                # parse, search, graph, types
  styles/
```

Scaffold a new section the same way: `pages/<name>/`, optional `pages/<name>/components/`, `lib/<name>/`, then mount under `<Route path="…">` in `app.tsx`.

## Deploy (GitHub Pages)

Production host: **https://auraspace.github.io/aura/**

Workflow: `.github/workflows/deploy-github-pages.yml` builds with `VITE_BASE=/aura/` and uploads `site/dist` as a GitHub Pages artifact.

### One-time GitHub setup

1. In repository **Settings → Pages**, set the source to **GitHub Actions**.
2. Push to `main`, or run **Actions → Deploy site (GitHub Pages)** manually.

Push to `main` (or **Actions → Deploy site (GitHub Pages) → Run workflow**) to publish.

### Public URLs

| Path                                                  | Page               |
| ----------------------------------------------------- | ------------------ |
| https://auraspace.github.io/aura/                     | Marketing homepage |
| https://auraspace.github.io/aura/docs                 | User guide hub     |
| https://auraspace.github.io/aura/docs/getting-started | Guide article      |
| https://auraspace.github.io/aura/rfc                  | RFC catalog        |
| https://auraspace.github.io/aura/rfc/000              | RFC-000 detail     |
| https://auraspace.github.io/aura/rfc/graph            | Dependency graph   |

Legacy `/graph` redirects to `/rfc/graph`.

## Content

| Source                 | Site route             |
| ---------------------- | ---------------------- |
| `../docs/guide/*.md`   | `/docs`, `/docs/:slug` |
| `../docs/rfc/RFC-*.md` | `/rfc`, `/rfc/:id`     |

Rebuild the site after editing guide or RFC markdown.
