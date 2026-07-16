import { getAllMeta } from '@/lib/rfc/load-rfcs'
import { buildGraph } from '@/lib/rfc/graph'
import { DepGraph } from '@/pages/rfc/components/dep-graph'
import { Link } from 'react-router-dom'

export function GraphPage() {
  const { nodes, edges } = buildGraph(getAllMeta())

  return (
    <div>
      <h1>RFC dependency graph</h1>
      <p className="text-muted">
        Edges from each RFC’s Depends and Blocks fields.
      </p>
      <DepGraph nodes={nodes} edges={edges} />
      <h2>Edge list</h2>
      <ul className="mt-6 text-[0.9rem] text-muted">
        {edges.map((e) => (
          <li key={`${e.kind}-${e.from}-${e.to}`}>
            <Link to={`/rfc/${e.from}`}>RFC-{e.from}</Link>{' '}
            {e.kind === 'depends' ? 'depends on' : 'blocks'}{' '}
            <Link to={`/rfc/${e.to}`}>RFC-{e.to}</Link>
          </li>
        ))}
      </ul>
    </div>
  )
}
