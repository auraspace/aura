import { useEffect, useState } from 'react'

type Theme = 'light' | 'dark'

function readTheme(): Theme {
  if (typeof document === 'undefined') return 'light'
  const attr = document.documentElement.getAttribute('data-theme')
  if (attr === 'dark' || attr === 'light') return attr
  return 'light'
}

function applyTheme(theme: Theme) {
  document.documentElement.setAttribute('data-theme', theme)
  try {
    localStorage.setItem('theme', theme)
  } catch {
    /* ignore */
  }
}

export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>('light')

  useEffect(() => {
    setTheme(readTheme())
  }, [])

  function toggle() {
    const next: Theme = theme === 'dark' ? 'light' : 'dark'
    setTheme(next)
    applyTheme(next)
  }

  return (
    <button
      type="button"
      className="cursor-pointer rounded-lg border border-border bg-bg px-2.5 py-1.5 text-sm text-fg"
      onClick={toggle}
      aria-label={`Switch to ${theme === 'dark' ? 'light' : 'dark'} mode`}
    >
      {theme === 'dark' ? 'Light' : 'Dark'}
    </button>
  )
}
