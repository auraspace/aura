# Aura RFC Static Site Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a Vite + React + TypeScript docs site under `site/` that parses `docs/rfc/*.md`, prerenders static HTML (readable without JS), selectively hydrates search/filter/theme/graph, and deploys to GitHub Pages.

**Architecture:** Build-time parse of RFC metadata tables → in-memory/content modules → React pages with `StaticRouter` prerender into `site/dist/**/index.html`. Client `BrowserRouter` hydrates interactive islands. Content source stays at `docs/rfc/` (read-only).

**Tech Stack:** pnpm, Vite 6, React 19, TypeScript, React Router 7, Vitest, `react-markdown` + `remark-gfm` + `rehype-slug`, MiniSearch, CSS variables (dark mode), custom SVG dependency graph, GitHub Actions Pages.

**Spec:** `docs/superpowers/specs/2026-07-15-rfc-static-site-design.md`

---

## File structure (lock-in)

```
site/
  package.json
  vite.config.ts
  tsconfig.json
  tsconfig.node.json
  index.html
  vitest.config.ts
  src/
    main.tsx                 # browser entry (hydrate)
    entry-server.tsx         # SSR render for prerender script
    App.tsx                  # routes shell
    vite-env.d.ts
    styles/global.css
    types/rfc.ts             # RfcStatus, RfcMeta, RfcDoc
    lib/
      parse-rfc.ts           # pure parse (no DOM)
      slugify.ts
      load-rfcs.ts           # import.meta.glob + parse all
      search.ts              # MiniSearch helpers
      graph.ts               # nodes/edges from meta
      links.ts               # rewrite RFC-xxx in markdown
    pages/
      HomePage.tsx
      RfcPage.tsx
      GraphPage.tsx
      NotFoundPage.tsx
    components/
      Header.tsx
      ThemeToggle.tsx
      StatusBadge.tsx
      LayerChip.tsx
      FilterBar.tsx
      SearchBox.tsx
      RfcCatalog.tsx
      RfcArticle.tsx
      Toc.tsx
      DepLinks.tsx
      DepGraph.tsx
      Layout.tsx
  scripts/
    prerender.ts             # renderToString → write dist HTML
  public/
    .gitkeep
  src/lib/parse-rfc.test.ts
  src/lib/slugify.test.ts
  src/lib/graph.test.ts

.github/workflows/deploy-site.yml
package.json                 # root: site:dev / site:build scripts (optional helper)
```

---

### Task 1: Scaffold `site/` Vite + React + TS

**Files:**
- Create: `site/package.json`, `site/vite.config.ts`, `site/tsconfig.json`, `site/tsconfig.node.json`, `site/index.html`, `site/src/main.tsx`, `site/src/App.tsx`, `site/src/vite-env.d.ts`, `site/src/styles/global.css`, `site/vitest.config.ts`
- Modify: root `package.json` (add site scripts only)

- [ ] **Step 1: Create Vite React-TS app in `site/`**

```bash
cd /Users/tienpham/Work/entj-pham/auraspace/aura
pnpm create vite site --template react-ts
cd site
pnpm install
pnpm add react-router-dom minisearch react-markdown remark-gfm rehype-slug rehype-raw
pnpm add -D vitest @types/node tsx
```

If `create vite` interactive prompts appear, use defaults for React + TypeScript.

- [ ] **Step 2: Configure Vite base + path to docs + vitest**

Replace `site/vite.config.ts`:

```ts
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'node:path'

const base = process.env.VITE_BASE || '/'

export default defineConfig({
  base,
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'src'),
    },
  },
  server: {
    fs: {
      // allow importing markdown from ../docs/rfc
      allow: [path.resolve(__dirname, '..')],
    },
  },
})
```

`site/vitest.config.ts`:

```ts
import { defineConfig } from 'vitest/config'
import path from 'node:path'

export default defineConfig({
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'src'),
    },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
})
```

`site/src/vite-env.d.ts` — ensure raw markdown modules:

```ts
/// <reference types="vite/client" />

declare module '*.md?raw' {
  const content: string
  export default content
}
```

- [ ] **Step 3: Wire package scripts in `site/package.json`**

```json
{
  "name": "aura-site",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build && tsx scripts/prerender.ts",
    "preview": "vite preview",
    "test": "vitest run",
    "test:watch": "vitest"
  }
}
```

(Prerender script lands in Task 7; until then temporary `"build": "tsc -b && vite build"` is OK — restore full script in Task 7.)

Root `package.json` scripts merge:

```json
{
  "scripts": {
    "site:dev": "pnpm --dir site dev",
    "site:build": "pnpm --dir site build",
    "site:test": "pnpm --dir site test",
    "test": "pnpm --dir site test"
  }
}
```

- [ ] **Step 4: Smoke dev server**

```bash
pnpm --dir site dev
```

Expected: Vite starts; default page loads. Stop with Ctrl+C.

- [ ] **Step 5: Commit**

```bash
git add site package.json pnpm-lock.yaml
git commit -m "chore(site): scaffold Vite React TypeScript app"
```

---

### Task 2: Types + slugify (TDD)

**Files:**
- Create: `site/src/types/rfc.ts`, `site/src/lib/slugify.ts`, `site/src/lib/slugify.test.ts`

- [ ] **Step 1: Write failing slugify tests**

`site/src/lib/slugify.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { slugify } from './slugify'

describe('slugify', () => {
  it('lowercases and hyphenates', () => {
    expect(slugify('Vision & Design Principles')).toBe('vision-design-principles')
  })

  it('strips punctuation', () => {
    expect(slugify('1. Abstract')).toBe('1-abstract')
  })

  it('collapses whitespace', () => {
    expect(slugify('  Hello   World  ')).toBe('hello-world')
  })
})
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
pnpm --dir site test
```

Expected: FAIL — cannot find module `./slugify` or `slugify` not exported.

- [ ] **Step 3: Implement slugify + types**

`site/src/lib/slugify.ts`:

```ts
export function slugify(text: string): string {
  return text
    .toLowerCase()
    .trim()
    .replace(/&/g, ' ')
    .replace(/[^\w\s-]/g, '')
    .replace(/[\s_]+/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '')
}
```

`site/src/types/rfc.ts`:

```ts
export type RfcStatus =
  | 'Draft'
  | 'In Review'
  | 'Accepted'
  | 'Frozen'
  | 'Rejected'
  | 'Superseded'

export interface RfcMeta {
  id: string
  slug: string
  title: string
  status: RfcStatus
  layer: string
  authors: string[]
  created?: string
  updated?: string
  estimate?: string
  depends: string[]
  blocks: string[]
  fileName: string
}

export interface Heading {
  depth: number
  text: string
  id: string
}

export interface RfcDoc extends RfcMeta {
  markdown: string
  headings: Heading[]
}
```

- [ ] **Step 4: Run tests — expect PASS**

```bash
pnpm --dir site test
```

Expected: all slugify tests PASS.

- [ ] **Step 5: Commit**

```bash
git add site/src/types/rfc.ts site/src/lib/slugify.ts site/src/lib/slugify.test.ts
git commit -m "feat(site): add Rfc types and slugify helper"
```

---

### Task 3: `parse-rfc` (TDD)

**Files:**
- Create: `site/src/lib/parse-rfc.ts`, `site/src/lib/parse-rfc.test.ts`

- [ ] **Step 1: Write failing parse tests**

`site/src/lib/parse-rfc.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { parseRfcMarkdown, parseDependsList } from './parse-rfc'

const SAMPLE = `# RFC-000: Vision & Design Principles

| Field        | Value                      |
| ------------ | -------------------------- |
| **RFC**      | 000                        |
| **Title**    | Vision & Design Principles |
| **Status**   | In Review                  |
| **Layer**    | Foundation                 |
| **Authors**  |                            |
| **Created**  | 2026-07-15                 |
| **Updated**  | 2026-07-15                 |
| **Estimate** | 15–20 pages                |
| **Depends**  | —                          |
| **Blocks**   | RFC-001 … RFC-003          |

---

## 1. Abstract

Hello body.

## 2. Motivation

More text.
`

describe('parseDependsList', () => {
  it('returns empty for em dash', () => {
    expect(parseDependsList('—')).toEqual([])
  })

  it('parses single RFC id', () => {
    expect(parseDependsList('RFC-004')).toEqual(['004'])
  })

  it('expands inclusive range with ellipsis', () => {
    expect(parseDependsList('RFC-001 … RFC-003')).toEqual(['001', '002', '003'])
  })

  it('parses comma-separated list', () => {
    expect(parseDependsList('RFC-001, RFC-002')).toEqual(['001', '002'])
  })
})

describe('parseRfcMarkdown', () => {
  it('extracts meta and body', () => {
    const doc = parseRfcMarkdown(SAMPLE, 'RFC-000-vision-design-principles.md')
    expect(doc.id).toBe('000')
    expect(doc.title).toBe('Vision & Design Principles')
    expect(doc.status).toBe('In Review')
    expect(doc.layer).toBe('Foundation')
    expect(doc.depends).toEqual([])
    expect(doc.blocks).toEqual(['001', '002', '003'])
    expect(doc.markdown).toContain('## 1. Abstract')
    expect(doc.markdown).not.toContain('**Status**')
    expect(doc.headings.map((h) => h.text)).toEqual(['1. Abstract', '2. Motivation'])
    expect(doc.headings[0].id).toBe('1-abstract')
    expect(doc.slug).toBe('rfc-000-vision-design-principles')
  })

  it('throws when RFC field missing', () => {
    const bad = `# Title\n\n| Field | Value |\n| **Status** | Draft |\n\n---\n\n## x\n`
    expect(() => parseRfcMarkdown(bad, 'bad.md')).toThrow(/RFC/)
  })
})
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
pnpm --dir site test
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement `parse-rfc.ts`**

```ts
import type { RfcDoc, RfcStatus } from '@/types/rfc'
import { slugify } from './slugify'

const VALID_STATUS = new Set<RfcStatus>([
  'Draft',
  'In Review',
  'Accepted',
  'Frozen',
  'Rejected',
  'Superseded',
])

function padId(n: number): string {
  return String(n).padStart(3, '0')
}

/** Parse Depends/Blocks cell into zero-padded ids. */
export function parseDependsList(raw: string): string[] {
  const text = raw.replace(/\u2026/g, '…').trim()
  if (!text || text === '—' || text === '-' || text === '–') return []

  const range = text.match(/RFC-(\d+)\s*[…\.]+?\s*RFC-(\d+)/i)
  if (range) {
    const start = parseInt(range[1], 10)
    const end = parseInt(range[2], 10)
    const out: string[] = []
    for (let i = start; i <= end; i++) out.push(padId(i))
    return out
  }

  const ids = [...text.matchAll(/RFC-(\d+)/gi)].map((m) => padId(parseInt(m[1], 10)))
  if (ids.length) return [...new Set(ids)]

  return [...text.matchAll(/\b(\d{1,3})\b/g)].map((m) => padId(parseInt(m[1], 10)))
}

function parseMetaTable(source: string): Record<string, string> {
  const fields: Record<string, string> = {}
  for (const line of source.split('\n')) {
    const m = line.match(/^\|\s*\*\*([^*]+)\*\*\s*\|\s*(.*?)\s*\|\s*$/)
    if (!m) continue
    fields[m[1].trim()] = m[2].trim()
  }
  return fields
}

function extractBody(source: string): string {
  const hr = source.indexOf('\n---\n')
  if (hr !== -1) return source.slice(hr + 5).trim() + '\n'

  // Fallback: after last metadata table row
  const lines = source.split('\n')
  let lastTable = -1
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].startsWith('|')) lastTable = i
  }
  if (lastTable === -1) return source
  return lines.slice(lastTable + 1).join('\n').trim() + '\n'
}

function extractHeadings(markdown: string) {
  const headings: { depth: number; text: string; id: string }[] = []
  for (const line of markdown.split('\n')) {
    const m = line.match(/^(#{2,3})\s+(.+)$/)
    if (!m) continue
    const text = m[2].replace(/#+\s*$/, '').trim()
    headings.push({ depth: m[1].length, text, id: slugify(text) })
  }
  return headings
}

export function parseRfcMarkdown(source: string, fileName: string): RfcDoc {
  const fields = parseMetaTable(source)
  const idRaw = fields['RFC']
  if (!idRaw) throw new Error(`Missing **RFC** field in ${fileName}`)
  const id = padId(parseInt(idRaw.replace(/\D/g, ''), 10))
  if (Number.isNaN(parseInt(id, 10))) throw new Error(`Invalid RFC id in ${fileName}`)

  const statusRaw = fields['Status']
  if (!statusRaw) throw new Error(`Missing **Status** field in ${fileName}`)
  if (!VALID_STATUS.has(statusRaw as RfcStatus)) {
    throw new Error(`Invalid status "${statusRaw}" in ${fileName}`)
  }

  const title =
    fields['Title'] ||
    source.match(/^#\s+RFC-\d+:\s*(.+)$/m)?.[1]?.trim() ||
    fileName

  const markdown = extractBody(source)
  const baseSlug = fileName.replace(/\.md$/i, '').toLowerCase()

  return {
    id,
    slug: baseSlug.startsWith('rfc-') ? baseSlug : `rfc-${id}-${slugify(title)}`,
    title,
    status: statusRaw as RfcStatus,
    layer: fields['Layer'] || 'Unknown',
    authors: (fields['Authors'] || '')
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean),
    created: fields['Created'] || undefined,
    updated: fields['Updated'] || undefined,
    estimate: fields['Estimate'] || undefined,
    depends: parseDependsList(fields['Depends'] || ''),
    blocks: parseDependsList(fields['Blocks'] || ''),
    fileName,
    markdown,
    headings: extractHeadings(markdown),
  }
}
```

- [ ] **Step 4: Run tests — fix until PASS**

```bash
pnpm --dir site test
```

Expected: all parse tests PASS. Adjust range regex if ellipsis character in fixtures differs (`…` U+2026 vs `...`).

- [ ] **Step 5: Commit**

```bash
git add site/src/lib/parse-rfc.ts site/src/lib/parse-rfc.test.ts
git commit -m "feat(site): parse RFC metadata tables and bodies"
```

---

### Task 4: Load all RFCs via `import.meta.glob`

**Files:**
- Create: `site/src/lib/load-rfcs.ts`
- Create: `site/src/lib/graph.ts`, `site/src/lib/graph.test.ts`

- [ ] **Step 1: Implement loader**

`site/src/lib/load-rfcs.ts`:

```ts
import { parseRfcMarkdown } from './parse-rfc'
import type { RfcDoc, RfcMeta } from '@/types/rfc'

const modules = import.meta.glob('../../../docs/rfc/RFC-*.md', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>

function fileNameFromPath(p: string): string {
  return p.split('/').pop() || p
}

export function loadAllRfcs(): RfcDoc[] {
  const docs: RfcDoc[] = []
  for (const [path, source] of Object.entries(modules)) {
    const fileName = fileNameFromPath(path)
    if (!/^RFC-\d+/i.test(fileName)) continue
    docs.push(parseRfcMarkdown(source, fileName))
  }
  docs.sort((a, b) => a.id.localeCompare(b.id))
  return docs
}

let cache: RfcDoc[] | null = null

export function getAllRfcs(): RfcDoc[] {
  if (!cache) cache = loadAllRfcs()
  return cache
}

export function getRfcById(id: string): RfcDoc | undefined {
  const pad = id.padStart(3, '0')
  return getAllRfcs().find((r) => r.id === pad)
}

export function getAllMeta(): RfcMeta[] {
  return getAllRfcs().map(({ markdown: _m, headings: _h, ...meta }) => meta)
}
```

- [ ] **Step 2: Graph helpers + test**

`site/src/lib/graph.ts`:

```ts
import type { RfcMeta } from '@/types/rfc'

export type GraphEdgeKind = 'depends' | 'blocks'

export interface GraphNode {
  id: string
  title: string
  status: string
  layer: string
}

export interface GraphEdge {
  from: string
  to: string
  kind: GraphEdgeKind
}

export function buildGraph(metas: RfcMeta[]): { nodes: GraphNode[]; edges: GraphEdge[] } {
  const nodes: GraphNode[] = metas.map((m) => ({
    id: m.id,
    title: m.title,
    status: m.status,
    layer: m.layer,
  }))
  const edges: GraphEdge[] = []
  for (const m of metas) {
    for (const d of m.depends) {
      edges.push({ from: m.id, to: d, kind: 'depends' }) // m depends on d
    }
    for (const b of m.blocks) {
      edges.push({ from: m.id, to: b, kind: 'blocks' }) // m blocks b
    }
  }
  return { nodes, edges }
}
```

`site/src/lib/graph.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { buildGraph } from './graph'
import type { RfcMeta } from '@/types/rfc'

const meta = (partial: Partial<RfcMeta> & Pick<RfcMeta, 'id'>): RfcMeta => ({
  slug: `rfc-${partial.id}`,
  title: partial.title || partial.id,
  status: 'Draft',
  layer: 'Language',
  authors: [],
  depends: [],
  blocks: [],
  fileName: `RFC-${partial.id}.md`,
  ...partial,
})

describe('buildGraph', () => {
  it('creates depends edges from child to parent', () => {
    const { edges } = buildGraph([
      meta({ id: '000', blocks: ['001'] }),
      meta({ id: '001', depends: ['000'] }),
    ])
    expect(edges).toContainEqual({ from: '001', to: '000', kind: 'depends' })
    expect(edges).toContainEqual({ from: '000', to: '001', kind: 'blocks' })
  })
})
```

- [ ] **Step 3: Run unit tests**

```bash
pnpm --dir site test
```

Expected: PASS (loader not unit-tested here; verified in dev Task 5).

- [ ] **Step 4: Commit**

```bash
git add site/src/lib/load-rfcs.ts site/src/lib/graph.ts site/src/lib/graph.test.ts
git commit -m "feat(site): load RFCs from docs and build dependency graph data"
```

---

### Task 5: App shell, theme, layout, routes (dev SPA)

**Files:**
- Create/replace: `site/src/styles/global.css`, `site/src/components/Header.tsx`, `site/src/components/ThemeToggle.tsx`, `site/src/components/Layout.tsx`, `site/src/components/StatusBadge.tsx`, `site/src/components/LayerChip.tsx`, `site/src/pages/HomePage.tsx` (stub), `site/src/pages/RfcPage.tsx` (stub), `site/src/pages/GraphPage.tsx` (stub), `site/src/pages/NotFoundPage.tsx`, `site/src/App.tsx`, `site/src/main.tsx`

- [ ] **Step 1: Global CSS with theme variables**

`site/src/styles/global.css` — include at least:

```css
:root,
[data-theme='light'] {
  --bg: #fafafa;
  --fg: #1a1a1a;
  --muted: #666;
  --border: #e0e0e0;
  --accent: #0b57d0;
  --card: #fff;
  --code-bg: #f4f4f5;
}

[data-theme='dark'] {
  --bg: #121212;
  --fg: #e8e8e8;
  --muted: #a0a0a0;
  --border: #333;
  --accent: #8ab4f8;
  --card: #1e1e1e;
  --code-bg: #2a2a2a;
}

* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: system-ui, -apple-system, Segoe UI, sans-serif;
  background: var(--bg);
  color: var(--fg);
  line-height: 1.55;
}
a { color: var(--accent); }
.layout { min-height: 100vh; display: flex; flex-direction: column; }
.header {
  display: flex; align-items: center; gap: 1rem;
  padding: 0.75rem 1.25rem; border-bottom: 1px solid var(--border);
  background: var(--card);
}
.header nav { display: flex; gap: 1rem; flex: 1; }
.header a { text-decoration: none; color: var(--fg); font-weight: 500; }
.header a:hover { color: var(--accent); }
.main { flex: 1; padding: 1.25rem; max-width: 1100px; width: 100%; margin: 0 auto; }
.badge {
  display: inline-block; font-size: 0.75rem; padding: 0.15rem 0.5rem;
  border-radius: 999px; border: 1px solid var(--border); color: var(--muted);
}
```

- [ ] **Step 2: ThemeToggle + Header + Layout**

`ThemeToggle.tsx`: on click toggles `document.documentElement.dataset.theme` between `light`/`dark`, persists `localStorage.theme`. On mount, read storage or `matchMedia('(prefers-color-scheme: dark)')`.

`Header.tsx`: brand “Aura RFCs”, links Home (`/`), Graph (`/graph`), `<ThemeToggle />`. Use `Link` from react-router-dom. Prefix paths with `import.meta.env.BASE_URL` via React Router `basename`.

`Layout.tsx`:

```tsx
import { Outlet } from 'react-router-dom'
import { Header } from './Header'

export function Layout() {
  return (
    <div className="layout">
      <Header />
      <main className="main">
        <Outlet />
      </main>
    </div>
  )
}
```

- [ ] **Step 3: App routes**

`App.tsx`:

```tsx
import { Routes, Route } from 'react-router-dom'
import { Layout } from './components/Layout'
import { HomePage } from './pages/HomePage'
import { RfcPage } from './pages/RfcPage'
import { GraphPage } from './pages/GraphPage'
import { NotFoundPage } from './pages/NotFoundPage'

export function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<HomePage />} />
        <Route path="rfc/:id" element={<RfcPage />} />
        <Route path="graph" element={<GraphPage />} />
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  )
}
```

`main.tsx`:

```tsx
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import { App } from './App'
import './styles/global.css'

const basename = import.meta.env.BASE_URL.replace(/\/$/, '') || '/'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <BrowserRouter basename={basename === '/' ? undefined : basename}>
      <App />
    </BrowserRouter>
  </StrictMode>,
)
```

Stub pages: Home shows `<h1>Aura RFCs</h1>` and `getAllMeta().length`; RfcPage shows `useParams().id`; GraphPage “Graph”; NotFound “Not found”.

- [ ] **Step 4: Verify dev + real RFC count**

```bash
pnpm --dir site dev
```

Open browser: Home should show count ≈ 14 (RFC-000…013). Fix glob path if 0.

- [ ] **Step 5: Commit**

```bash
git add site/src
git commit -m "feat(site): app shell, theme, and routes"
```

---

### Task 6: Catalog + RFC detail (static-friendly markup)

**Files:**
- Create: `site/src/components/RfcCatalog.tsx`, `site/src/components/FilterBar.tsx`, `site/src/components/SearchBox.tsx`, `site/src/components/RfcArticle.tsx`, `site/src/components/Toc.tsx`, `site/src/components/DepLinks.tsx`, `site/src/lib/search.ts`, `site/src/lib/links.ts`
- Modify: `site/src/pages/HomePage.tsx`, `site/src/pages/RfcPage.tsx`, `site/src/styles/global.css`

- [ ] **Step 1: StatusBadge, DepLinks, RfcCatalog (SSR-safe list)**

`RfcCatalog` props: `items: RfcMeta[]` — render `<table>` with columns RFC, Title, Status, Layer, links to `rfc/${id}`.

`DepLinks`: given `depends` / `blocks` id arrays, render `Link` to each existing RFC or `<span className="missing">` if unknown.

- [ ] **Step 2: Markdown article**

`RfcArticle.tsx`:

```tsx
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeSlug from 'rehype-slug'
import type { RfcDoc } from '@/types/rfc'

export function RfcArticle({ doc }: { doc: RfcDoc }) {
  return (
    <article className="rfc-article">
      <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeSlug]}>
        {doc.markdown}
      </ReactMarkdown>
    </article>
  )
}
```

`Toc.tsx`: list `doc.headings` as `<a href={\`#${id}\`}>`.

`RfcPage.tsx`: load `getRfcById(params.id)`; 404 if missing; show title, StatusBadge, LayerChip, DepLinks, two-column layout (Toc | Article).

- [ ] **Step 3: Filter + Search (client state; still render full list in HTML)**

`FilterBar`: controlled `status` + `layer` selects; options derived from data.

`SearchBox`: controlled query string.

`HomePage` logic:

```tsx
// Always map full list into DOM for no-JS readability.
// When JS runs, hide non-matching rows via filter (class or filter style).
```

Implementation approach that keeps no-JS content:

1. Render **all** rows in the table (static).
2. Hydrated filter/search sets `data-hidden="true"` on non-matching `<tr>` via `useEffect` / controlled filter — when JS off, all rows visible.

`search.ts`:

```ts
import MiniSearch from 'minisearch'
import type { RfcDoc } from '@/types/rfc'

export function buildSearchIndex(docs: RfcDoc[]) {
  const mini = new MiniSearch({
    fields: ['title', 'status', 'layer', 'body'],
    storeFields: ['id'],
    idField: 'id',
  })
  mini.addAll(
    docs.map((d) => ({
      id: d.id,
      title: d.title,
      status: d.status,
      layer: d.layer,
      body: d.markdown.replace(/```[\s\S]*?```/g, ' ').slice(0, 20000),
    })),
  )
  return mini
}
```

- [ ] **Step 4: Manual check**

```bash
pnpm --dir site dev
```

- Home lists all RFCs; filter Draft reduces visible rows; search “LLVM” finds relevant RFC.
- Open `/rfc/000` — body + TOC; click TOC jumps.

- [ ] **Step 5: Commit**

```bash
git add site/src
git commit -m "feat(site): RFC catalog, detail view, filter and search"
```

---

### Task 7: Prerender static HTML (SSG)

**Files:**
- Create: `site/src/entry-server.tsx`, `site/scripts/prerender.ts`
- Modify: `site/package.json` build script, `site/index.html`

- [ ] **Step 1: Server entry**

`site/src/entry-server.tsx`:

```tsx
import { renderToString } from 'react-dom/server'
import { StaticRouter } from 'react-router-dom'
import { App } from './App'
import './styles/global.css'

export function render(url: string, basename: string) {
  const html = renderToString(
    <StaticRouter location={url} basename={basename === '/' ? undefined : basename}>
      <App />
    </StaticRouter>,
  )
  return html
}
```

Note: React Router v7 may export `StaticRouter` from `react-router-dom/server` — check installed version and import accordingly:

```ts
import { StaticRouter } from 'react-router-dom/server'
```

- [ ] **Step 2: Prerender script**

`site/scripts/prerender.ts`:

```ts
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { loadAllRfcs } from '../src/lib/load-rfcs.ts'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const dist = path.resolve(__dirname, '../dist')
const template = fs.readFileSync(path.join(dist, 'index.html'), 'utf8')
const base = (process.env.VITE_BASE || '/').replace(/\/$/, '') || ''
const basename = base || '/'

// Dynamic import of server render after vite build bundled it —
// Prefer vite-node or build entry-server as SSR bundle.
// Practical approach: use tsx + react directly (same as below via entry-server).

const { render } = await import(
  pathToFileURL(path.resolve(__dirname, '../src/entry-server.tsx')).href
)

const rfcs = loadAllRfcs()
const routes = ['/', '/graph', ...rfcs.map((r) => `/rfc/${r.id}`)]

function writeRoute(route: string, appHtml: string) {
  const out =
    route === '/'
      ? path.join(dist, 'index.html')
      : path.join(dist, route.replace(/^\//, ''), 'index.html')
  fs.mkdirSync(path.dirname(out), { recursive: true })
  const page = template.replace(
    '<div id="root"></div>',
    `<div id="root">${appHtml}</div>`,
  )
  fs.writeFileSync(out, page)
  console.log('prerender', route, '→', path.relative(dist, out))
}

for (const route of routes) {
  const html = render(route, basename === '/' ? '/' : basename)
  writeRoute(route, html)
}

// GH Pages SPA fallback for client navigations to unknown paths
fs.copyFileSync(path.join(dist, 'index.html'), path.join(dist, '404.html'))
console.log('done', routes.length, 'routes')
```

If `tsx` cannot import `.tsx` with CSS side-effects, switch prerender to a Vite SSR build:

```ts
// vite.config.ts add ssr build in script instead — implementer may use:
// pnpm exec vite build --ssr src/entry-server.tsx --outDir dist-ssr
// then import from dist-ssr/entry-server.js
```

**Canonical working pattern (use if Step 2 fails):**

1. `vite build` (client)
2. `vite build --ssr src/entry-server.tsx --outDir dist-ssr`
3. `node scripts/prerender.mjs` imports `../dist-ssr/entry-server.js` and `loadAllRfcs` from a small prebundled graph of meta written during client build

Implementer: pick the path that works with the installed Vite version; acceptance is `dist/rfc/000/index.html` contains `Vision` or `Abstract` text.

Update `package.json`:

```json
"build": "tsc -b && vite build && tsx scripts/prerender.ts"
```

or the two-step vite SSR variant documented in a comment at top of `prerender.ts`.

- [ ] **Step 3: Build and verify static content**

```bash
pnpm --dir site build
# should include article text without needing JS:
grep -l "Abstract" site/dist/rfc/000/index.html || grep -i "vision" site/dist/rfc/000/index.html
wc -c site/dist/rfc/000/index.html
```

Expected: file exists, size > ~2KB, contains RFC content strings.

Also:

```bash
grep -c "RFC-00" site/dist/index.html
```

Expected: multiple RFC links present.

- [ ] **Step 4: Commit**

```bash
git add site/scripts site/src/entry-server.tsx site/package.json site/vite.config.ts
git commit -m "feat(site): prerender static HTML for all RFC routes"
```

---

### Task 8: Dependency graph page

**Files:**
- Create: `site/src/components/DepGraph.tsx`
- Modify: `site/src/pages/GraphPage.tsx`, `site/src/styles/global.css`

- [ ] **Step 1: SVG graph (no heavy lib)**

`DepGraph.tsx`:
- Input: `nodes`, `edges` from `buildGraph(getAllMeta())`
- Layout: simple grid/circle layout by sorting ids (or layer columns: Foundation | Language | Toolchain | Runtime)
- Draw `<line>` for edges (depends solid, blocks dashed)
- Draw `<Link>`-wrapped node labels
- Noscript fallback: GraphPage also renders a plain `<ul>` of “RFC-x depends on …” so static HTML has relations without JS

- [ ] **Step 2: Wire GraphPage**

```tsx
import { getAllMeta } from '@/lib/load-rfcs'
import { buildGraph } from '@/lib/graph'
import { DepGraph } from '@/components/DepGraph'

export function GraphPage() {
  const { nodes, edges } = buildGraph(getAllMeta())
  return (
    <div>
      <h1>RFC dependency graph</h1>
      <DepGraph nodes={nodes} edges={edges} />
      <ul className="graph-fallback">
        {edges.map((e) => (
          <li key={`${e.kind}-${e.from}-${e.to}`}>
            RFC-{e.from} {e.kind === 'depends' ? 'depends on' : 'blocks'} RFC-{e.to}
          </li>
        ))}
      </ul>
    </div>
  )
}
```

- [ ] **Step 3: Manual + build check**

```bash
pnpm --dir site dev   # /graph interactive
pnpm --dir site build
grep "depends on" site/dist/graph/index.html
```

- [ ] **Step 4: Commit**

```bash
git add site/src/components/DepGraph.tsx site/src/pages/GraphPage.tsx site/src/styles/global.css
git commit -m "feat(site): RFC dependency graph page"
```

---

### Task 9: Polish — deep links, internal RFC links, a11y

**Files:**
- Create: `site/src/lib/links.ts`
- Modify: `RfcArticle.tsx`, `global.css`, maybe `Toc.tsx`

- [ ] **Step 1: Rewrite `RFC-XXX` mentions in markdown to links (optional rehype/remark)**

In `RfcArticle`, custom `components` for `a` and text, or preprocess markdown:

```ts
export function linkifyRfcRefs(md: string): string {
  return md.replace(
    /\bRFC-(\d{3})\b/g,
    (full, id, offset, str) => {
      // skip if already inside a markdown link
      const before = str.slice(Math.max(0, offset - 1), offset)
      if (before === '[') return full
      return `[${full}](rfc/${id})`
    },
  )
}
```

Pass `linkifyRfcRefs(doc.markdown)` into ReactMarkdown. Use `basename`-aware links via React Router `Link` only for app routes; relative `rfc/000` from `/graph` is wrong — prefer absolute path `/rfc/000` with ReactMarkdown `components.a` using `Link` when `href` starts with `/rfc/` or `rfc/`.

- [ ] **Step 2: Scroll to hash on load**

In `RfcPage` `useEffect`: if `location.hash`, `document.getElementById` scrollIntoView.

- [ ] **Step 3: Commit**

```bash
git add site/src
git commit -m "feat(site): RFC cross-links and hash deep-linking"
```

---

### Task 10: GitHub Pages workflow + root scripts

**Files:**
- Create: `.github/workflows/deploy-site.yml`
- Modify: root `package.json` if not done

- [ ] **Step 1: Workflow**

`.github/workflows/deploy-site.yml`:

```yaml
name: Deploy RFC site

on:
  push:
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: true

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm
          cache-dependency-path: site/pnpm-lock.yaml
      - name: Install
        working-directory: site
        run: pnpm install
      - name: Build
        working-directory: site
        env:
          VITE_BASE: /${{ github.event.repository.name }}/
        run: pnpm build
      - uses: actions/upload-pages-artifact@v3
        with:
          path: site/dist

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - id: deployment
        uses: actions/deploy-pages@v4
```

If monorepo uses root lockfile only, point `cache-dependency-path` to root `pnpm-lock.yaml` and run `pnpm install` from root with `pnpm --dir site build`.

- [ ] **Step 2: Document enable Pages**

Add short note to `site/README.md`:

```markdown
# Aura RFC site

## Dev
pnpm --dir site dev

## Build
pnpm --dir site build

## GitHub Pages
Repo Settings → Pages → Source: GitHub Actions.
Site base path is \`/<repo-name>/\`.
```

- [ ] **Step 3: Final verification**

```bash
pnpm --dir site test
pnpm --dir site build
test -f site/dist/index.html
test -f site/dist/rfc/000/index.html
test -f site/dist/graph/index.html
```

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/deploy-site.yml site/README.md package.json
git commit -m "ci(site): deploy static RFC site to GitHub Pages"
```

---

## Self-review (plan vs spec)

| Spec requirement | Task |
| ---------------- | ---- |
| Vite + React + TS in `site/` | 1 |
| Parse metadata table RFC-*.md | 3 |
| Skip TEMPLATE/README | 4 (glob RFC- only) |
| Catalog + status/layer | 6 |
| Search MiniSearch | 6 |
| Depends/Blocks links | 6 |
| Dependency graph | 8 |
| Dark mode | 5 |
| Deep-link headings | 6 + 9 |
| SSG HTML readable without JS | 7 |
| Selective hydrate | 5–6, 8 |
| GitHub Pages + base | 1, 7, 10 |
| Fail build on missing RFC/Status | 3 (throw) + 4 load |
| English UI | all copy in components |
| Unit tests parse | 2, 3, 4 |

No TBD placeholders remain for core path; prerender notes include fallback SSR build if `tsx`+CSS import fails.

---

## Execution handoff

Plan saved to `docs/superpowers/plans/2026-07-15-rfc-static-site.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks  
2. **Inline Execution** — this session, executing-plans, batch with checkpoints  

Which approach?
