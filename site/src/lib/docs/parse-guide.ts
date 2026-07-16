import GithubSlugger from 'github-slugger'
import { plainHeadingText } from '@/lib/markdown/heading-text'
import type { GuideDoc, GuideHeading } from './types'

function fileSlug(fileName: string): string {
  return fileName.replace(/\.md$/i, '')
}

/** Minimal `key: value` YAML-like frontmatter (no nested structures). */
export function parseFrontmatter(source: string): {
  fields: Record<string, string>
  body: string
} {
  const normalized = source.replace(/^\uFEFF/, '')
  if (!normalized.startsWith('---\n') && !normalized.startsWith('---\r\n')) {
    return { fields: {}, body: normalized }
  }

  const end = normalized.indexOf('\n---', 4)
  if (end === -1) return { fields: {}, body: normalized }

  const raw = normalized.slice(4, end)
  const after = normalized.slice(end + 4).replace(/^\r?\n*/, '')
  const fields: Record<string, string> = {}

  for (const line of raw.split(/\r?\n/)) {
    const m = line.match(/^([A-Za-z][\w-]*)\s*:\s*(.*)$/)
    if (!m) continue
    let value = m[2].trim()
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1)
    }
    fields[m[1]] = value
  }

  return { fields, body: after }
}

function extractHeadings(markdown: string): GuideHeading[] {
  const slugger = new GithubSlugger()
  const headings: GuideHeading[] = []
  let inFence = false

  for (const line of markdown.split('\n')) {
    if (/^```/.test(line)) {
      inFence = !inFence
      continue
    }
    if (inFence) continue

    const m = line.match(/^(#{2,3})\s+(.+?)\s*#*\s*$/)
    if (!m) continue
    // Keep inline MD in `text` for TOC rendering; slug from plain text
    // so ids match rehype-slug (github-slugger on textContent).
    const text = m[2].replace(/\s+#+\s*$/, '').trim()
    const plain = plainHeadingText(text)
    headings.push({
      depth: m[1].length,
      text,
      id: slugger.slug(plain),
    })
  }

  return headings
}

/** Drop a leading H1 that duplicates the page title (shown in the shell). */
export function stripLeadingH1(markdown: string): string {
  return markdown.replace(/^#\s+[^\n]+\n+/, '')
}

export function parseGuideMarkdown(source: string, fileName: string): GuideDoc {
  const { fields, body } = parseFrontmatter(source)
  const slug = fields.slug?.trim() || fileSlug(fileName)
  const title = fields.title?.trim() || slug
  const section = fields.section?.trim() || 'Guide'
  const order = Number.parseInt(fields.order ?? '100', 10)
  const summary = fields.summary?.trim() || ''

  const markdown = stripLeadingH1(body.trim() + '\n')
  const headings = extractHeadings(markdown)

  if (!slug) {
    throw new Error(`Guide ${fileName}: missing slug`)
  }

  return {
    slug,
    title,
    section,
    order: Number.isFinite(order) ? order : 100,
    summary,
    fileName,
    markdown,
    headings,
  }
}
