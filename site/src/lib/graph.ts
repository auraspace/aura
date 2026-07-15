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

export function buildGraph(metas: RfcMeta[]): {
  nodes: GraphNode[]
  edges: GraphEdge[]
} {
  const nodes: GraphNode[] = metas.map((m) => ({
    id: m.id,
    title: m.title,
    status: m.status,
    layer: m.layer,
  }))
  const edges: GraphEdge[] = []
  for (const m of metas) {
    for (const d of m.depends) {
      edges.push({ from: m.id, to: d, kind: 'depends' })
    }
    for (const b of m.blocks) {
      edges.push({ from: m.id, to: b, kind: 'blocks' })
    }
  }
  return { nodes, edges }
}
