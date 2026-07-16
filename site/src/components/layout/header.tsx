import { Link } from 'react-router-dom'
import { ThemeToggle } from './theme-toggle'

export function Header() {
  return (
    <header className="sticky top-0 z-10 flex items-center gap-4 border-b border-border bg-card px-5 py-3">
      <Link
        to="/rfc"
        className="shrink-0 whitespace-nowrap font-bold text-fg no-underline"
      >
        Aura RFCs
      </Link>
      <nav className="flex flex-1 gap-4">
        <Link
          to="/rfc"
          className="font-medium text-fg no-underline hover:text-accent"
        >
          RFCs
        </Link>
        <Link
          to="/rfc/graph"
          className="font-medium text-fg no-underline hover:text-accent"
        >
          Graph
        </Link>
      </nav>
      <ThemeToggle />
    </header>
  )
}
