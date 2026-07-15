# Design: Aura RFC Static Site (Vite + React + SSG)

| Field | Value |
| ----- | ----- |
| **Date** | 2026-07-15 |
| **Status** | Approved (brainstorm) |
| **Stack** | Vite + React + TypeScript |
| **App path** | `site/` |
| **Content source** | `docs/rfc/*.md` (read-only) |
| **Deploy** | GitHub Pages |
| **UI language** | English |

## 1. Goals

Build an **internal team docs site** that:

1. Parses Aura RFCs from `docs/rfc/`.
2. **Pre-renders static HTML** per route (readable with JS disabled for article body).
3. **Selectively hydrates** interactive widgets (search, filter, dark mode, dependency graph).
4. Deploys to **GitHub Pages** from `site/dist`.

### Non-goals (MVP)

- i18n UI, in-browser RFC editing, auth, comments
- Server-side search or backend API
- Mutating RFC source files during build
- Application frameworks beyond docs viewing

## 2. Product decisions

| Axis | Choice |
| ---- | ------ |
| Audience | Internal team |
| Feature depth | Full MVP: catalog, filter, search, Depends/Blocks links, dependency graph, dark mode, heading deep-links |
| Location | `site/` at repo root |
| Chrome UI language | English (RFC bodies remain English) |
| Rendering | SSG + selective hydration (not pure SPA shell, not zero-JS) |

## 3. Architecture

```
docs/rfc/*.md                 ← source of truth (unchanged)
        │
        ▼
site/                         ← Vite + React + TS
  src/lib/parse-rfc.ts        ← shared parse logic
  scripts/ingest (optional)   ← generate metadata artifacts
  src/pages/                  ← Home, RfcDetail, Graph
  src/components/             ← UI units
        │
        ▼  pnpm build
site/dist/                    ← static HTML + assets
        │
        ▼
GitHub Pages                  ← Actions upload dist; base = /<repo>/
```

### Responsibilities

| Unit | Responsibility |
| ---- | -------------- |
| `docs/rfc` | RFC markdown only |
| `site` | UI, ingest, SSG build |
| Build pipeline | Read-only parse; fail hard on missing required meta fields |

### Rendering model

- **Static (no JS required):** catalog listing links, full RFC HTML body, TOC anchors, Depends/Blocks text links.
- **Hydrated (JS):** search (MiniSearch), status/layer filters, dark mode toggle, interactive dependency graph, optional TOC scroll-spy.

## 4. Data model & RFC parse

### Sources

- Glob: `docs/rfc/RFC-*.md`
- Exclude: `TEMPLATE.md`, `README.md`

### On-disk format

RFCs use a **metadata table** after the H1 (not YAML frontmatter), for example:

```markdown
# RFC-000: Vision & Design Principles

| Field        | Value                      |
| **RFC**      | 000                        |
| **Title**    | Vision & Design Principles |
| **Status**   | In Review                  |
| **Layer**    | Foundation                 |
| **Depends**  | —                          |
| **Blocks**   | RFC-001 … RFC-013          |
```

Body begins after the first `---` separator (or after the metadata table if no separator).

### TypeScript types

```ts
type RfcStatus =
  | 'Draft'
  | 'In Review'
  | 'Accepted'
  | 'Frozen'
  | 'Rejected'
  | 'Superseded'

interface RfcMeta {
  id: string // "000"
  slug: string // "rfc-000-vision-design-principles"
  title: string
  status: RfcStatus
  layer: string
  authors: string[]
  created?: string
  updated?: string
  estimate?: string
  depends: string[] // normalized ids, e.g. ["001"]
  blocks: string[]
  fileName: string
}

interface RfcDoc extends RfcMeta {
  markdown: string
  headings: { depth: number; text: string; id: string }[]
}
```

### Parse pipeline

1. Read UTF-8 file.
2. Title from H1 and/or **Title** field (prefer field when both present; keep consistent).
3. Parse Field/Value table → `RfcMeta`.
4. Normalize Depends/Blocks tokens (`RFC-001`, `001`); map `—` / empty → `[]`. Expand explicit ranges only when pattern is unambiguous (`RFC-001 … RFC-013` → ids `001`…`013`).
5. Body = content after first `---` (fallback: after metadata table).
6. Extract `##` / `###` headings → TOC ids via slugify.
7. Emit build artifacts:
   - List meta for catalog / graph / search seed (`rfcs.json` or equivalent generated module).
   - Per-RFC content available at build for prerender (glob or per-file JSON).

### Search

- Client **MiniSearch** over `id`, `title`, `status`, `layer`, and plain-text body (code blocks may be stripped or down-weighted).
- Index built at hydrate time from generated list, or prebuilt `search-index.json` in `dist`.

### Graph

- Nodes: all `RfcMeta`.
- Edges: from `depends` and `blocks` (UI labels direction clearly: “depends on” vs “blocks”).

### Parse errors

- Missing required **RFC** or **Status** → **fail build**.
- Depends/Blocks pointing at unknown id → **build warning**; still ship; link may show missing.

## 5. Routes & UI

### Routes (prerendered)

| Path | Dist output (example) | Content |
| ---- | --------------------- | ------- |
| `/` | `index.html` | Catalog, filter shell, search shell |
| `/rfc/:id` | `rfc/000/index.html` | Full article + TOC + meta |
| `/graph` | `graph/index.html` | Dependency graph shell |
| 404 | `404.html` | Simple not-found (and/or Pages fallback) |

- `id` is zero-padded three digits (`000`).
- Display label: `RFC-000`.
- React Router paths honor Vite `base` (e.g. `/aura/`).
- Prerender route list is derived from generated meta at build time (not runtime crawl).

### Layout

```
┌─────────────────────────────────────────────┐
│ Header: Aura RFCs | Home | Graph | theme    │
├──────────┬──────────────────────────────────┤
│ Sidebar  │  Main                            │
│ TOC on   │  catalog / article / graph       │
│ detail   │                                  │
└──────────┴──────────────────────────────────┘
```

### Components

| Component | Static HTML | Client JS |
| --------- | ----------- | --------- |
| `Header` + nav | yes | theme toggle |
| `RfcCatalog` | list + links | filter + search |
| `StatusBadge`, `LayerChip` | yes | — |
| `RfcArticle` | full body HTML | — |
| `Toc` | anchor links | optional scroll-spy |
| `DepLinks` | links | — |
| `SearchBox` | shell | MiniSearch |
| `FilterBar` | shell | client state |
| `DepGraph` | noscript node list optional | interactive graph |
| `ThemeToggle` | — | `localStorage` + `data-theme` |

### Markdown

- GFM (tables, fences, lists).
- Heading ids = slug (match TOC and `#` deep links).
- Rewrite internal `RFC-00x` / relative `.md` links to `/rfc/00x` at build.
- Prefer **build-time** syntax highlighting (e.g. Shiki) so code blocks look correct without JS.

### Styling

- Global CSS + CSS variables (or CSS modules).
- Dark mode via `data-theme` on `<html>`; default `prefers-color-scheme`.
- No heavy component library required for MVP.

## 6. Tooling layout

```
site/
  package.json          # aura-site
  vite.config.ts        # base, prerender
  tsconfig.json
  index.html
  src/main.tsx
  src/App.tsx
  src/pages/
  src/components/
  src/lib/parse-rfc.ts
  src/generated/        # build output of ingest (gitignored or committed—prefer generate on build)
  scripts/              # optional ingest CLI
  public/
```

Root optional scripts:

- `site:dev` → `pnpm --dir site dev`
- `site:build` → `pnpm --dir site build`

Package manager: **pnpm** (repo already uses it).

## 7. Build pipeline

```text
pnpm --dir site build
  1. Ingest docs/rfc → meta (+ optional search index)
  2. vite build (JS/CSS assets)
  3. Prerender routes → dist/**/index.html
```

Success criteria:

1. Article HTML readable with JS disabled.
2. With JS: filter, search, dark mode, graph work.
3. GH Pages: `/rfc/000/` and heading hashes work with correct `base`.

## 8. GitHub Pages

- Build on push to `main` via GitHub Actions.
- Artifact: `site/dist`.
- `base` / `VITE_BASE` = `/${{ github.event.repository.name }}/` (e.g. `/aura/`).
- `dist/` remains gitignored (root `.gitignore` already covers `dist/`).

## 9. Testing (MVP)

- Unit tests for `parse-rfc` (table extract, depends normalize, range expand, skip template).
- Smoke: build succeeds; spot-check that `dist/rfc/000/index.html` contains expected title/body fragment.

## 10. Implementation order (high level)

1. Scaffold `site/` (Vite React TS), base config, GH Pages-ready `base`.
2. Implement `parse-rfc` + fixtures from real RFCs; tests green.
3. Ingest → generated meta; Home catalog static list.
4. RfcDetail page + markdown → HTML + TOC + deep links.
5. Prerender all routes into `dist`.
6. Filter + search hydrate; theme toggle.
7. Graph page; Depends/Blocks cross-links.
8. GitHub Actions workflow; verify Pages path.

## 11. Risks & mitigations

| Risk | Mitigation |
| ---- | ---------- |
| Metadata table format drifts | Fail build on required fields; document parser expectations |
| GH Pages base path breaks assets/routes | Single `base` env; test with `vite preview --base` |
| Large markdown bundles | Per-route prerender; load only needed doc for detail |
| Graph library weight | Prefer light SVG/custom layout if React Flow is too heavy |
