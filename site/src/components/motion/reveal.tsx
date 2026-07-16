import {
  motion,
  type HTMLMotionProps,
  type Transition,
  type Variants,
} from 'motion/react'
import type { ReactNode } from 'react'

/** Vochi-style ease-out (cubic-bezier(0.16, 1, 0.3, 1)). */
export const easeOutExpo: Transition['ease'] = [0.16, 1, 0.3, 1]

export const fadeUp: Variants = {
  hidden: { opacity: 0, y: 16 },
  show: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.7, ease: easeOutExpo },
  },
}

export const fadeUpSoft: Variants = {
  hidden: { opacity: 0, y: 12 },
  show: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.65, ease: easeOutExpo },
  },
}

export const stagger: Variants = {
  hidden: {},
  show: {
    transition: {
      staggerChildren: 0.08,
      delayChildren: 0.04,
    },
  },
}

type RevealProps = {
  children: ReactNode
  className?: string
  delay?: number
  y?: number
  /** Animate on mount (hero). Default: whileInView. */
  onMount?: boolean
  once?: boolean
} & Omit<HTMLMotionProps<'div'>, 'children' | 'initial' | 'animate' | 'whileInView'>

/**
 * Fade + rise reveal — same pattern as vochi.xyz (Motion + easeOutExpo).
 */
export function Reveal({
  children,
  className,
  delay = 0,
  y = 16,
  onMount = false,
  once = true,
  ...rest
}: RevealProps) {
  const transition: Transition = {
    duration: 0.7,
    delay,
    ease: easeOutExpo,
  }

  const hidden = { opacity: 0, y }
  const show = { opacity: 1, y: 0 }

  if (onMount) {
    return (
      <motion.div
        className={className}
        initial={hidden}
        animate={show}
        transition={transition}
        {...rest}
      >
        {children}
      </motion.div>
    )
  }

  return (
    <motion.div
      className={className}
      initial={hidden}
      whileInView={show}
      viewport={{ once, amount: 0.2, margin: '0px 0px -40px 0px' }}
      transition={transition}
      {...rest}
    >
      {children}
    </motion.div>
  )
}

type StaggerProps = {
  children: ReactNode
  className?: string
  staggerDelay?: number
  delayChildren?: number
} & Omit<
  HTMLMotionProps<'div'>,
  'children' | 'initial' | 'whileInView' | 'variants'
>

export function Stagger({
  children,
  className,
  staggerDelay = 0.08,
  delayChildren = 0.04,
  ...rest
}: StaggerProps) {
  return (
    <motion.div
      className={className}
      initial="hidden"
      whileInView="show"
      viewport={{ once: true, amount: 0.15, margin: '0px 0px -40px 0px' }}
      variants={{
        hidden: {},
        show: {
          transition: {
            staggerChildren: staggerDelay,
            delayChildren,
          },
        },
      }}
      {...rest}
    >
      {children}
    </motion.div>
  )
}

export function StaggerItem({
  children,
  className,
  y = 14,
  ...rest
}: {
  children: ReactNode
  className?: string
  y?: number
} & Omit<HTMLMotionProps<'div'>, 'children' | 'variants'>) {
  return (
    <motion.div
      className={className}
      variants={{
        hidden: { opacity: 0, y },
        show: {
          opacity: 1,
          y: 0,
          transition: { duration: 0.65, ease: easeOutExpo },
        },
      }}
      {...rest}
    >
      {children}
    </motion.div>
  )
}
