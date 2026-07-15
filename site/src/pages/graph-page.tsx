import { getAllMeta } from '@/lib/load-rfcs'
import { buildGraph } from '@/lib/graph'
import { DepGraph } from '@/components/dep-graph'
import { Link } from 'react-router-dom'

export function GraphPage() {
  const { nodes, edges } = buildGraph(getAllMeta())

  return (
    <div>
      <h1>RFC dependency graph</h1>
      <p className="muted">
        Edges from each RFC’s Depends and Blocks fields.
      </p>
      <DepGraph nodes={nodes} edges={edges} />
      <h2>Edge list</h2>
      <ul className="graph-fallback">
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
