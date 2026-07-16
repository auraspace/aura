import { Link } from 'react-router-dom'
import type { GraphEdge, GraphNode } from '@/lib/rfc/graph'

const LAYER_ORDER = [
  'Foundation',
  'Language',
  'Toolchain',
  'Runtime',
  'Framework',
]

interface Pos {
  x: number
  y: number
}

function layout(nodes: GraphNode[]): Map<string, Pos> {
  const byLayer = new Map<string, GraphNode[]>()
  for (const n of nodes) {
    const layer = LAYER_ORDER.includes(n.layer) ? n.layer : 'Other'
    const list = byLayer.get(layer) || []
    list.push(n)
    byLayer.set(layer, list)
  }

  const cols = [
    ...LAYER_ORDER.filter((l) => byLayer.has(l)),
    ...(byLayer.has('Other') ? ['Other'] : []),
  ]

  const pos = new Map<string, Pos>()
  const colW = 180
  const rowH = 56
  const padX = 40
  const padY = 40

  cols.forEach((layer, ci) => {
    const list = (byLayer.get(layer) || []).sort((a, b) =>
      a.id.localeCompare(b.id),
    )
    list.forEach((n, ri) => {
      pos.set(n.id, { x: padX + ci * colW, y: padY + ri * rowH })
    })
  })

  return pos
}

export function DepGraph({
  nodes,
  edges,
}: {
  nodes: GraphNode[]
  edges: GraphEdge[]
}) {
  const pos = layout(nodes)
  let maxX = 400
  let maxY = 200
  for (const p of pos.values()) {
    maxX = Math.max(maxX, p.x + 140)
    maxY = Math.max(maxY, p.y + 50)
  }

  return (
    <div className="overflow-auto rounded-lg border border-border bg-card p-4">
      <svg
        className="block h-auto w-full min-w-[720px]"
        viewBox={`0 0 ${maxX} ${maxY}`}
        role="img"
        aria-label="RFC dependency graph"
      >
        {edges.map((e) => {
          const a = pos.get(e.from)
          const b = pos.get(e.to)
          if (!a || !b) return null
          const x1 = a.x + 60
          const y1 = a.y + 18
          const x2 = b.x + 60
          const y2 = b.y + 18
          return (
            <line
              key={`${e.kind}-${e.from}-${e.to}`}
              x1={x1}
              y1={y1}
              x2={x2}
              y2={y2}
              className={
                e.kind === 'depends' ? 'graph-edge-depends' : 'graph-edge-blocks'
              }
            />
          )
        })}
        {nodes.map((n) => {
          const p = pos.get(n.id)
          if (!p) return null
          return (
            <g key={n.id} className="graph-node" transform={`translate(${p.x},${p.y})`}>
              <Link to={`/rfc/${n.id}`}>
                <rect width="120" height="36" rx="6" />
                <text x="60" y="22" textAnchor="middle">
                  RFC-{n.id}
                </text>
              </Link>
            </g>
          )
        })}
      </svg>
      <p className="mt-3 text-[0.85rem] text-muted">
        Solid = depends on · Dashed = blocks
      </p>
    </div>
  )
}
