import { useEffect, useRef } from 'react'

interface Glyph {
  x: number
  y: number
  vx: number
  vy: number
  text: string
  size: number
  alpha: number
  baseAlpha: number
  angle: number
  spin: number
  color: string
}

interface Ripple {
  x: number
  y: number
  radius: number
  maxRadius: number
  alpha: number
}

const SYMBOLS = [
  'fun',
  'val',
  'var',
  'class',
  'interface',
  'async',
  'spawn',
  'join',
  'build',
  'null',
  '{ }',
  '->',
  '::',
  'T?',
  '01',
  'Aura',
  '✔',
  '0x4A',
]

export function AuraCanvas() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return

    const ctx = canvas.getContext('2d')
    if (!ctx) return

    let animationFrameId: number
    let width = (canvas.width =
      canvas.parentElement?.clientWidth || window.innerWidth)
    let height = (canvas.height = canvas.parentElement?.clientHeight || 600)

    const glyphs: Glyph[] = []
    const ripples: Ripple[] = []

    const glyphCount = Math.min(Math.floor(width / 24), 50)

    let mouseX = -1000
    let mouseY = -1000
    let lastMouseX = -1000
    let lastMouseY = -1000

    const initGlyphs = () => {
      glyphs.length = 0
      for (let i = 0; i < glyphCount; i++) {
        const isHighlight = Math.random() < 0.25
        glyphs.push({
          x: Math.random() * width,
          y: Math.random() * height,
          vx: (Math.random() - 0.5) * 0.5,
          vy: (Math.random() - 0.5) * 0.5,
          text: SYMBOLS[Math.floor(Math.random() * SYMBOLS.length)],
          size: Math.floor(Math.random() * 6) + 11, // 11px - 17px
          alpha: Math.random() * 0.45 + 0.15,
          baseAlpha: Math.random() * 0.45 + 0.15,
          angle: (Math.random() - 0.5) * 0.2,
          spin: (Math.random() - 0.5) * 0.005,
          color: isHighlight ? 'accent' : 'muted',
        })
      }
    }

    const handleResize = () => {
      if (!canvas.parentElement) return
      width = canvas.width = canvas.parentElement.clientWidth
      height = canvas.height = canvas.parentElement.clientHeight
      initGlyphs()
    }

    const handleMouseMove = (e: MouseEvent) => {
      const rect = canvas.getBoundingClientRect()
      mouseX = e.clientX - rect.left
      mouseY = e.clientY - rect.top

      // Add a subtle motion trail ripple when moving mouse fast
      const dist = Math.hypot(mouseX - lastMouseX, mouseY - lastMouseY)
      if (dist > 35) {
        ripples.push({
          x: mouseX,
          y: mouseY,
          radius: 10,
          maxRadius: 70,
          alpha: 0.5,
        })
        lastMouseX = mouseX
        lastMouseY = mouseY
      }
    }

    const handleMouseLeave = () => {
      mouseX = -1000
      mouseY = -1000
    }

    const handleClick = (e: MouseEvent) => {
      const rect = canvas.getBoundingClientRect()
      const cx = e.clientX - rect.left
      const cy = e.clientY - rect.top

      // Click shockwave
      ripples.push({
        x: cx,
        y: cy,
        radius: 10,
        maxRadius: 180,
        alpha: 0.8,
      })

      // Push glyphs away from click
      for (const g of glyphs) {
        const dx = g.x - cx
        const dy = g.y - cy
        const dist = Math.hypot(dx, dy)
        if (dist < 180) {
          const force = (1 - dist / 180) * 8
          g.vx += (dx / (dist || 1)) * force
          g.vy += (dy / (dist || 1)) * force
        }
      }
    }

    const parent = canvas.parentElement
    window.addEventListener('resize', handleResize)
    parent?.addEventListener('mousemove', handleMouseMove)
    parent?.addEventListener('mouseleave', handleMouseLeave)
    parent?.addEventListener('click', handleClick)

    initGlyphs()

    let time = 0

    const render = () => {
      time += 0.02
      ctx.clearRect(0, 0, width, height)

      const isDark =
        document.documentElement.getAttribute('data-theme') === 'dark'
      const accentRgb = isDark ? '95, 191, 130' : '44, 117, 72'
      const fgRgb = isDark ? '236, 234, 228' : '15, 19, 17'

      // 1. Draw Shockwave & Motion Ripples
      for (let i = ripples.length - 1; i >= 0; i--) {
        const r = ripples[i]
        r.radius += 2.5
        r.alpha *= 0.94

        if (r.alpha <= 0.01 || r.radius >= r.maxRadius) {
          ripples.splice(i, 1)
          continue
        }

        ctx.beginPath()
        ctx.arc(r.x, r.y, r.radius, 0, Math.PI * 2)
        ctx.strokeStyle = `rgba(${accentRgb}, ${r.alpha})`
        ctx.lineWidth = 1.5
        ctx.stroke()
      }

      // 2. Render Interactive Floating Code Glyphs
      ctx.font = '12px "IBM Plex Mono", monospace'

      for (let i = 0; i < glyphs.length; i++) {
        const g = glyphs[i]

        // Velocity damping & truing
        g.x += g.vx
        g.y += g.vy
        g.vx *= 0.98
        g.vy *= 0.98
        g.angle += g.spin

        // Add subtle floating sin wave
        g.y += Math.sin(time + i) * 0.15

        // Boundary wrap
        if (g.x < -30) g.x = width + 30
        if (g.x > width + 30) g.x = -30
        if (g.y < -30) g.y = height + 30
        if (g.y > height + 30) g.y = -30

        // Mouse magnetic repulsion & glowing interaction
        const dx = mouseX - g.x
        const dy = mouseY - g.y
        const dist = Math.hypot(dx, dy)

        let isHovered = false
        if (dist < 130) {
          isHovered = true
          const force = (1 - dist / 130) * 0.6
          g.x -= (dx / dist) * force * 2.5
          g.y -= (dy / dist) * force * 2.5

          // Draw connection beam to mouse
          ctx.beginPath()
          ctx.moveTo(g.x, g.y)
          ctx.lineTo(mouseX, mouseY)
          ctx.strokeStyle = `rgba(${accentRgb}, ${(1 - dist / 130) * 0.25})`
          ctx.lineWidth = 0.8
          ctx.stroke()
        }

        // Render Glyph text
        ctx.save()
        ctx.translate(g.x, g.y)
        ctx.rotate(g.angle)

        ctx.font = `${g.size}px "IBM Plex Mono", monospace`

        const colorStr =
          g.color === 'accent' || isHovered
            ? `rgba(${accentRgb}, ${isHovered ? 0.95 : g.alpha})`
            : `rgba(${fgRgb}, ${g.alpha * 0.6})`

        ctx.fillStyle = colorStr

        if (isHovered || g.color === 'accent') {
          ctx.shadowColor = `rgba(${accentRgb}, 0.6)`
          ctx.shadowBlur = isHovered ? 12 : 6
        }

        ctx.fillText(g.text, 0, 0)
        ctx.restore()
      }

      animationFrameId = requestAnimationFrame(render)
    }

    render()

    return () => {
      cancelAnimationFrame(animationFrameId)
      window.removeEventListener('resize', handleResize)
      parent?.removeEventListener('mousemove', handleMouseMove)
      parent?.removeEventListener('mouseleave', handleMouseLeave)
      parent?.removeEventListener('click', handleClick)
    }
  }, [])

  return (
    <canvas
      ref={canvasRef}
      className="absolute inset-0 pointer-events-none z-0 opacity-80"
    />
  )
}
