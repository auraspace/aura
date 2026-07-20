import {
  IconBrandGithub,
  IconExternalLink,
  IconMenu2,
  IconX,
} from '@tabler/icons-react'
import { useEffect, useId, useState } from 'react'
import { Link, useLocation } from 'react-router-dom'

import { ThemeToggle } from './theme-toggle'

const NAV_LINKS = [
  { to: { pathname: '/', hash: 'features' } as const, label: 'Features' },
  { to: '/docs' as const, label: 'Docs' },
  { to: '/rfc' as const, label: 'RFCs' },
  { to: '/rfc/graph' as const, label: 'Graph' },
]

export function Header() {
  const { pathname } = useLocation()
  const [scrolled, setScrolled] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)
  const menuId = useId()
  const isHome = pathname === '/'

  useEffect(() => {
    function onScroll() {
      setScrolled(window.scrollY > 8)
    }
    onScroll()
    window.addEventListener('scroll', onScroll, { passive: true })
    return () => window.removeEventListener('scroll', onScroll)
  }, [])

  // Close mobile menu on navigation.
  useEffect(() => {
    setMenuOpen(false)
  }, [pathname])

  useEffect(() => {
    if (!menuOpen) return
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') setMenuOpen(false)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [menuOpen])

  // Prevent body scroll while the menu is open.
  useEffect(() => {
    if (!menuOpen) return
    const prev = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    return () => {
      document.body.style.overflow = prev
    }
  }, [menuOpen])

  const headerSolid = scrolled || !isHome || menuOpen

  return (
    <header
      className={[
        'sticky top-0 z-40 transition-[background-color,box-shadow,border-color] duration-300',
        headerSolid
          ? 'border-b border-border bg-[var(--header-bg)] backdrop-blur-md'
          : 'border-b border-transparent bg-transparent',
      ].join(' ')}
      style={{ height: 72 }}
    >
      <div className="mx-auto flex h-full max-w-[1280px] items-center justify-between px-6">
        <Link
          to="/"
          className="group flex items-center gap-2.5 text-fg no-underline"
          onClick={() => setMenuOpen(false)}
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

        <nav className="hidden items-center gap-8 md:flex" aria-label="Primary">
          {NAV_LINKS.map((item) => (
            <Link key={item.label} to={item.to} className="navlink">
              {item.label}
            </Link>
          ))}
          <a
            href="https://github.com/auraspace/aura"
            className="navlink inline-flex items-center gap-1.5"
            rel="noreferrer"
            target="_blank"
          >
            <IconBrandGithub size={16} stroke={1.75} aria-hidden />
            GitHub
            <IconExternalLink
              size={14}
              stroke={1.75}
              aria-hidden
              className="opacity-60"
            />
          </a>
        </nav>

        <div className="flex items-center gap-2 sm:gap-3">
          <ThemeToggle />
          <Link
            to="/docs"
            className="hidden rounded-full bg-fg px-4 py-2 text-[13px] font-medium text-bg no-underline transition-colors hover:bg-accent-deep hover:text-card md:inline-block"
          >
            Read docs
          </Link>
          <button
            type="button"
            className="grid h-9 w-9 place-items-center rounded-full border border-border-strong bg-card text-fg transition-colors hover:border-accent md:hidden"
            aria-label={menuOpen ? 'Close menu' : 'Open menu'}
            aria-expanded={menuOpen}
            aria-controls={menuId}
            onClick={() => setMenuOpen((o) => !o)}
          >
            {menuOpen ? (
              <IconX size={18} stroke={1.75} aria-hidden />
            ) : (
              <IconMenu2 size={18} stroke={1.75} aria-hidden />
            )}
          </button>
        </div>
      </div>

      {menuOpen ? (
        <div
          id={menuId}
          className="absolute inset-x-0 top-full border-b border-border bg-[var(--header-bg)] backdrop-blur-md md:hidden"
          role="dialog"
          aria-label="Mobile navigation"
        >
          <nav className="mx-auto flex max-w-[1280px] flex-col gap-1 px-6 py-4">
            {NAV_LINKS.map((item) => (
              <Link
                key={item.label}
                to={item.to}
                className="rounded-lg px-3 py-2.5 text-[15px] font-medium text-fg no-underline transition-colors hover:bg-tint"
                onClick={() => setMenuOpen(false)}
              >
                {item.label}
              </Link>
            ))}
            <a
              href="https://github.com/auraspace/aura"
              className="inline-flex items-center gap-1.5 rounded-lg px-3 py-2.5 text-[15px] font-medium text-fg no-underline transition-colors hover:bg-tint"
              rel="noreferrer"
              target="_blank"
              onClick={() => setMenuOpen(false)}
            >
              <IconBrandGithub size={16} stroke={1.75} aria-hidden />
              GitHub
              <IconExternalLink
                size={14}
                stroke={1.75}
                aria-hidden
                className="opacity-60"
              />
            </a>
            <Link
              to="/docs"
              className="mt-2 rounded-full bg-fg px-4 py-2.5 text-center text-[13px] font-medium text-bg no-underline transition-colors hover:bg-accent-deep hover:text-card"
              onClick={() => setMenuOpen(false)}
            >
              Read docs
            </Link>
          </nav>
        </div>
      ) : null}
    </header>
  )
}
