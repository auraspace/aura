import { Link } from 'react-router-dom'
import { ThemeToggle } from './theme-toggle'

export function Header() {
  return (
    <header className="header">
      <Link to="/" className="brand">
        Aura RFCs
      </Link>
      <nav>
        <Link to="/">Home</Link>
        <Link to="/graph">Graph</Link>
      </nav>
      <ThemeToggle />
    </header>
  )
}
