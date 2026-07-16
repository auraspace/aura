import { getAllMeta } from '@/lib/rfc/load-rfcs'
import { buildGraph } from '@/lib/rfc/graph'
import { DepGraph } from '@/pages/rfc/components/dep-graph'
import { Link } from 'react-router-dom'

export function GraphPage() {
  const { nodes, edges } = buildGraph(getAllMeta())

  return (
    <main className="page-shell">
      <p className="eyebrow">Dependencies</p>
      <h1 className="mt-3 font-display text-[34px] font-medium tracking-tight md:text-[40px]">
        RFC graph
      </h1>
      <p className="mt-2 max-w-[520px] text-muted">
        Edges from each RFC’s Depends and Blocks fields.
      </p>
      <DepGraph nodes={nodes} edges={edges} />
      <h2 className="mt-10 font-display text-[22px] font-medium tracking-tight">
        Edge list
      </h2>
      <ul className="mt-4 space-y-1.5 text-[0.9rem] text-muted">
        {edges.map((e) => (
          <li key={`${e.kind}-${e.from}-${e.to}`}>
            <Link to={`/rfc/${e.from}`}>RFC-{e.from}</Link>{' '}
            {e.kind === 'depends' ? 'depends on' : 'blocks'}{' '}
            <Link to={`/rfc/${e.to}`}>RFC-{e.to}</Link>
          </li>
        ))}
      </ul>
    </main>
  )
}
