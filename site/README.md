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

Production host: **https://aura.fadosoft.com**

Workflow: `.github/workflows/deploy-site.yml` builds with `VITE_BASE=/` and uploads `site/dist` via Wrangler Direct Upload.

### One-time Cloudflare + GitHub setup

1. **Cloudflare API token**
   - My Profile → API Tokens → Create Token
   - Template **Edit Cloudflare Workers** (or custom: Account → Cloudflare Pages → Edit)
   - Save as repo secret `CLOUDFLARE_API_TOKEN`

2. **Account ID**
   - Cloudflare dashboard → Workers & Pages (or any domain overview) → **Account ID**
   - Repo secret `CLOUDFLARE_ACCOUNT_ID`

3. **GitHub secrets** on environment **`static-pages`**  
   (repo **Settings → Environments → static-pages → Environment secrets**):
   - `CLOUDFLARE_API_TOKEN`
   - `CLOUDFLARE_ACCOUNT_ID`  
     Workflow job uses `environment: static-pages` so secrets must live there (not only as repo secrets, unless you also mirror them).

4. **Pages project `aura`**
   - The deploy workflow runs `wrangler pages project create aura` on first deploy (no-op if it already exists).
   - Token needs **Account → Cloudflare Pages → Edit** (or the Workers template above).

5. **Custom domain** `aura.fadosoft.com`
   - After the first successful deploy: Workers & Pages → project **`aura`** → **Custom domains** → Add `aura.fadosoft.com`
   - DNS: **CNAME** `aura` → `aura.pages.dev` (or the target Cloudflare shows), proxy **ON** if the zone is on Cloudflare

6. Optional: turn off GitHub Pages (Settings → Pages → None) so `*.github.io` stops updating.

Push to `main` (or **Actions → Deploy site → Run workflow**) to publish.

### Public URLs

| Path                                           | Page               |
| ---------------------------------------------- | ------------------ |
| https://aura.fadosoft.com/                     | Marketing homepage |
| https://aura.fadosoft.com/docs                 | User guide hub     |
| https://aura.fadosoft.com/docs/getting-started | Guide article      |
| https://aura.fadosoft.com/rfc                  | RFC catalog        |
| https://aura.fadosoft.com/rfc/000              | RFC-000 detail     |
| https://aura.fadosoft.com/rfc/graph            | Dependency graph   |

Also available: `https://aura.pages.dev` until the custom domain is live.

Legacy `/graph` redirects to `/rfc/graph`.

## Content

| Source                 | Site route             |
| ---------------------- | ---------------------- |
| `../docs/guide/*.md`   | `/docs`, `/docs/:slug` |
| `../docs/rfc/RFC-*.md` | `/rfc`, `/rfc/:id`     |

Rebuild the site after editing guide or RFC markdown.
