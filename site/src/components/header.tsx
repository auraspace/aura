import { Link } from 'react-router-dom'
import { ThemeToggle } from './theme-toggle'

export function Header() {
  return (
    <header className="header">
      <Link to="/rfc" className="brand">
        Aura RFCs
      </Link>
      <nav>
        <Link to="/rfc">RFCs</Link>
        <Link to="/graph">Graph</Link>
      </nav>
      <ThemeToggle />
    </header>
  )
}
