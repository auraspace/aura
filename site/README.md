# Aura site

Marketing homepage, user guide (`docs/guide/` → `/docs`), and RFC catalog (`docs/rfc/` → `/rfc`). Built with Vite + React + Tailwind CSS v4; prerenders HTML for **Cloudflare Pages**.

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

## Deploy (Cloudflare Pages)

Production host: **https://aura.pilotworks.dev/**

Workflow: `.github/workflows/deploy-site.yml` builds with `VITE_BASE=/` and deploys `site/dist` to the `aura` Cloudflare Pages project.

### One-time GitHub setup

1. Add `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` as repository secrets.
2. Push to `main`, or run **Actions → Deploy site (Cloudflare Pages)** manually.

Push to `main` (or **Actions → Deploy site (Cloudflare Pages) → Run workflow**) to publish.

### Public URLs

| Path                                             | Page               |
| ------------------------------------------------ | ------------------ |
| https://aura.pilotworks.dev/                     | Marketing homepage |
| https://aura.pilotworks.dev/docs                 | User guide hub     |
| https://aura.pilotworks.dev/docs/getting-started | Guide article      |
| https://aura.pilotworks.dev/rfc                  | RFC catalog        |
| https://aura.pilotworks.dev/rfc/000              | RFC-000 detail     |
| https://aura.pilotworks.dev/rfc/graph            | Dependency graph   |

Legacy `/graph` redirects to `/rfc/graph`.

## Content

| Source                 | Site route             |
| ---------------------- | ---------------------- |
| `../docs/guide/*.md`   | `/docs`, `/docs/:slug` |
| `../docs/rfc/RFC-*.md` | `/rfc`, `/rfc/:id`     |

Rebuild the site after editing guide or RFC markdown.
