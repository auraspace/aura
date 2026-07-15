import { describe, it, expect } from 'vitest'
import { buildGraph } from './graph'
import type { RfcMeta } from '@/types/rfc'

const meta = (
  partial: Partial<RfcMeta> & Pick<RfcMeta, 'id'>,
): RfcMeta => ({
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
