import { IconBrandGithub, IconExternalLink } from '@tabler/icons-react'
import { useEffect, useState } from 'react'
import { Link, useLocation } from 'react-router-dom'
import { ThemeToggle } from './theme-toggle'

export function Header() {
  const { pathname } = useLocation()
  const [scrolled, setScrolled] = useState(false)
  const isHome = pathname === '/'

  useEffect(() => {
    function onScroll() {
      setScrolled(window.scrollY > 8)
    }
    onScroll()
    window.addEventListener('scroll', onScroll, { passive: true })
    return () => window.removeEventListener('scroll', onScroll)
  }, [])

  return (
    <header
      className={[
        'sticky top-0 z-40 transition-[background-color,box-shadow,border-color] duration-300',
        scrolled || !isHome
          ? 'border-b border-border bg-[var(--header-bg)] backdrop-blur-md'
          : 'border-b border-transparent bg-transparent',
      ].join(' ')}
      style={{ height: 72 }}
    >
      <div className="mx-auto flex h-full max-w-[1280px] items-center justify-between px-6">
        <Link
          to="/"
          className="group flex items-center gap-2.5 text-fg no-underline"
        >
          <img
            src={`${import.meta.env.BASE_URL}logo.svg`}
            alt=""
            width={28}
            height={28}
            className="h-7 w-7"
          />
          <span className="font-display text-[18px] font-medium tracking-tight">
            Aura
          </span>
        </Link>

        <nav className="hidden items-center gap-8 md:flex">
          <Link to={{ pathname: '/', hash: 'features' }} className="navlink">
            Features
          </Link>
          <Link to="/rfc" className="navlink">
            RFCs
          </Link>
          <Link to="/rfc/graph" className="navlink">
            Graph
          </Link>
          <a
            href="https://github.com/auraspace/aura"
            className="navlink inline-flex items-center gap-1.5"
            rel="noreferrer"
            target="_blank"
          >
            <IconBrandGithub size={16} stroke={1.75} aria-hidden />
            GitHub
            <IconExternalLink size={14} stroke={1.75} aria-hidden className="opacity-60" />
          </a>
        </nav>

        <div className="flex items-center gap-3">
          <ThemeToggle />
          <Link
            to="/rfc"
            className="hidden rounded-full bg-fg px-4 py-2 text-[13px] font-medium text-bg no-underline transition-colors hover:bg-accent-deep hover:text-card md:inline-block"
          >
            Read RFCs
          </Link>
        </div>
      </div>
    </header>
  )
}
