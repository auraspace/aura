# Aura RFC site

Static docs site for Aura RFCs (`docs/rfc/`). Built with Vite + React; prerenders HTML for GitHub Pages.

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

## GitHub Pages

1. Repo **Settings → Pages → Source: GitHub Actions**
2. Workflow: `.github/workflows/deploy-site.yml`
3. Base path is `/<repo-name>/` (set via `VITE_BASE` in CI)

Public URLs (repo `aura`):

| Path | Page |
| ---- | ---- |
| https://auraspace.github.io/aura/rfc | RFC catalog |
| https://auraspace.github.io/aura/rfc/000 | RFC-000 detail |
| https://auraspace.github.io/aura/graph | Dependency graph |

## Content

RFC markdown stays in `../docs/rfc/RFC-*.md` (read-only). Rebuild the site after editing RFCs.
