import {
  IconArrowRight,
  IconBinaryTree2,
  IconBrandGithub,
  IconCheck,
  IconCircleCheck,
  IconComponents,
  IconExternalLink,
  IconFileText,
  IconPackage,
  IconPlayerPlay,
  IconShieldCheck,
  IconTerminal2,
  type Icon,
} from '@tabler/icons-react'
import { motion, MotionConfig } from 'motion/react'
import { Link } from 'react-router-dom'

import {
  easeOutExpo,
  Reveal,
  Stagger,
  StaggerItem,
} from '@/components/motion/reveal'
import { getAllMeta } from '@/lib/rfc/load-rfcs'

const PILLARS: {
  title: string
  body: string
  Icon: Icon
}[] = [
  {
    title: 'Familiar on purpose',
    body: 'Classes, interfaces, methods, generics, and value types make application code readable without importing a framework first.',
    Icon: IconComponents,
  },
  {
    title: 'Null safety in the type system',
    body: 'Values are non-null by default. Optional values are explicit, and flow-sensitive checks keep the safe path direct.',
    Icon: IconShieldCheck,
  },
  {
    title: 'A runtime that stays with the program',
    body: 'GC and runtime support link into the executable, so the machine running your service does not need a separate Aura install.',
    Icon: IconPackage,
  },
  {
    title: 'Concurrency with visible boundaries',
    body: 'Spawn, join, cancellation, and bounded channels are explicit. Today the executor is deliberately single-threaded and deterministic.',
    Icon: IconBinaryTree2,
  },
]

const WORKFLOW: {
  command: string
  title: string
  body: string
}[] = [
  {
    command: 'aura check ./service',
    title: 'Catch problems early',
    body: 'Parse and typecheck the whole package with source-aware diagnostics.',
  },
  {
    command: 'aura test ./service',
    title: 'Test through the same toolchain',
    body: 'Run package tests without adding a separate task runner.',
  },
  {
    command: 'aura build ./service -o service',
    title: 'Produce a native executable',
    body: 'The C backend compiles your package and links the Aura runtime into the artifact.',
  },
  {
    command: './service',
    title: 'Run it without Aura installed',
    body: 'Copy the executable to a machine or place it in a small container image.',
  },
]

const WORKS_TODAY = [
  {
    t: 'Check, build, run, test, format, and emit C from one CLI.',
  },
  {
    t: 'Build multi-file packages with aura.toml and locked dependencies.',
  },
  {
    t: 'Get human-readable diagnostics or structured JSON for tooling.',
  },
  {
    t: 'Compile real corpus programs and repository examples through the C backend.',
  },
] as const

const COMES_LATER = [
  'LLVM as the production backend',
  'General async lowering and multi-threaded scheduling',
  'A complete hosted package registry and toolchain manager',
] as const

function HeroCodeCard() {
  return (
    <div className="float-y relative mx-auto w-full max-w-[380px] md:ml-auto md:mr-0">
      <div className="lift-md relative rounded-[28px] border border-border-strong bg-card p-5">
        <div className="flex items-center justify-between pb-4">
          <span className="font-mono text-[11px] text-muted">source</span>
          <span className="eyebrow inline-flex items-center gap-1 text-ink-muted">
            <IconTerminal2 size={12} stroke={1.75} aria-hidden />
            hello.aura
          </span>
        </div>

        <div className="overflow-hidden rounded-2xl border border-border bg-tint p-4 font-mono text-[12.5px] leading-[1.65]">
          <div>
            <span className="text-accent">package</span> main
          </div>
          <div className="mt-2">
            <span className="text-accent">fun</span> main() {'{'}
          </div>
          <div className="pl-4">
            println(<span className="text-fg">"Hello, Aura"</span>)
          </div>
          <div>{'}'}</div>
        </div>

        <div className="mt-4 rounded-xl border border-border bg-bg px-4 py-3">
          <div className="flex items-baseline justify-between gap-3">
            <span className="font-display text-[18px] tracking-tight">
              native executable
            </span>
            <span className="font-mono text-[11px] text-ink-muted">ready</span>
          </div>
          <div className="mt-1 flex items-center justify-between gap-2">
            <span className="font-mono text-[11px] text-muted">
              aura build hello.aura
            </span>
            <span className="inline-flex items-center gap-1 font-mono text-[11px] text-accent">
              <IconCheck size={12} stroke={2} aria-hidden />
              ok
            </span>
          </div>
        </div>

        <div className="mt-4 flex gap-2">
          <span className="inline-flex items-center gap-1.5 rounded-full border border-border bg-bg px-3 py-1.5 font-mono text-[10px] uppercase tracking-[0.12em] text-muted">
            <IconShieldCheck size={12} stroke={1.75} aria-hidden />
            check
          </span>
          <span className="inline-flex items-center gap-1.5 rounded-full border border-border bg-bg px-3 py-1.5 font-mono text-[10px] uppercase tracking-[0.12em] text-muted">
            <IconPackage size={12} stroke={1.75} aria-hidden />
            build
          </span>
          <span className="inline-flex items-center gap-1.5 rounded-full border border-border bg-fg px-3 py-1.5 font-mono text-[10px] uppercase tracking-[0.12em] text-bg">
            <IconPlayerPlay size={12} stroke={1.75} aria-hidden />
            run
          </span>
        </div>
      </div>
    </div>
  )
}

export function HomePage() {
  const rfcCount = getAllMeta().length

  return (
    <MotionConfig reducedMotion="user">
      <div className="relative flex-1">
        {/* Hero */}
        <section className="relative pb-16 pt-10 md:pb-24 md:pt-14">
          <div className="home-section grid grid-cols-1 items-center gap-14 md:grid-cols-12 md:gap-10">
            <div className="md:col-span-7">
              <Reveal onMount y={8} delay={0.02}>
                <p className="eyebrow">A compiled language for service code</p>
              </Reveal>

              <h1 className="mt-7 font-display text-[40px] leading-[1.05] font-medium tracking-tight text-balance md:text-[68px] md:leading-[1.02]">
                <Reveal onMount y={16} delay={0.08} className="block">
                  Easy to write.
                </Reveal>
                <Reveal
                  onMount
                  y={16}
                  delay={0.14}
                  className="block italic text-muted"
                >
                  Boring to deploy.
                </Reveal>
              </h1>

              <Reveal onMount y={12} delay={0.2}>
                <p className="mt-7 max-w-[520px] text-pretty text-[17px] leading-[1.55] text-muted md:text-[18px]">
                  Aura combines familiar classes, null-safe types, GC, and
                  native builds in one Rust-powered toolchain.
                </p>
              </Reveal>

              <Reveal onMount y={10} delay={0.26}>
                <div className="mt-9 flex flex-wrap items-center gap-4">
                  <Link to="/docs" className="btn-primary">
                    Read the docs
                    <IconArrowRight size={16} stroke={1.75} aria-hidden />
                  </Link>
                  <Link to="/rfc" className="btn-ghost">
                    Browse RFCs
                    <IconArrowRight size={15} stroke={1.75} aria-hidden />
                  </Link>
                </div>
              </Reveal>
            </div>

            <Reveal onMount y={16} delay={0.18} className="md:col-span-5">
              <HeroCodeCard />
            </Reveal>
          </div>
        </section>

        {/* Product promise */}
        <section
          id="features"
          className="border-t border-border py-20 md:py-24"
        >
          <div className="home-section">
            <Reveal y={12}>
              <h2 className="mt-4 max-w-[720px] font-display text-[34px] leading-[1.1] font-medium tracking-tight text-balance md:text-[48px]">
                Productive language design,
                <span className="italic text-muted">
                  {' '}
                  without outsourcing the deploy story.
                </span>
              </h2>
              <p className="mt-6 max-w-[620px] text-[17px] leading-[1.65] text-muted">
                Aura keeps application code approachable while making the build
                artifact explicit from the start.
              </p>
            </Reveal>

            <Stagger className="mt-14 grid grid-cols-1 gap-6 md:grid-cols-2">
              {PILLARS.map((pillar, index) => (
                <StaggerItem key={pillar.title}>
                  <article
                    className={`h-full rounded-2xl border border-border p-7 ${
                      index === 0 || index === 3 ? 'bg-tint/70' : 'bg-card'
                    }`}
                  >
                    <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-full border border-border bg-card text-accent">
                      <pillar.Icon size={20} stroke={1.5} aria-hidden />
                    </div>
                    <h3 className="font-display text-[23px] leading-snug tracking-tight">
                      {pillar.title}
                    </h3>
                    <p className="mt-3 text-[15px] leading-[1.55] text-muted">
                      {pillar.body}
                    </p>
                  </article>
                </StaggerItem>
              ))}
            </Stagger>
          </div>
        </section>

        {/* Deploy path */}
        <section className="border-t border-border bg-tint/60 py-20 md:py-28">
          <div className="home-section">
            <Reveal y={14}>
              <p className="eyebrow">The deploy path</p>
              <h2 className="mt-4 max-w-[700px] font-display text-[34px] leading-[1.1] font-medium tracking-tight text-balance md:text-[48px]">
                One toolchain from first check
                <span className="italic text-muted"> to running process.</span>
              </h2>
            </Reveal>

            <Stagger className="mt-14 grid grid-cols-1 gap-px overflow-hidden rounded-2xl border border-border bg-border md:grid-cols-2">
              {WORKFLOW.map((step) => (
                <StaggerItem key={step.command}>
                  <article className="h-full bg-card p-7 md:p-8">
                    <code className="font-mono text-[13px] text-accent">
                      $ {step.command}
                    </code>
                    <h3 className="mt-6 font-display text-[23px] leading-snug tracking-tight">
                      {step.title}
                    </h3>
                    <p className="mt-3 max-w-[420px] text-[15px] leading-[1.6] text-muted">
                      {step.body}
                    </p>
                  </article>
                </StaggerItem>
              ))}
            </Stagger>
          </div>
        </section>

        {/* Origin */}
        <section className="border-t border-border py-20 md:py-24">
          <div className="home-section grid grid-cols-1 gap-12 md:grid-cols-12 md:gap-10">
            <Reveal y={14} className="md:col-span-5">
              <h2 className="font-display text-[34px] leading-[1.1] font-medium tracking-tight text-balance md:text-[44px]">
                Why build another language?
              </h2>
              <p className="mt-6 max-w-[430px] text-[17px] leading-[1.65] text-muted">
                Because application developers should not have to choose between
                approachable code and a deployment model they can fully explain.
              </p>
            </Reveal>

            <Reveal
              y={14}
              delay={0.08}
              className="md:col-span-6 md:col-start-7"
            >
              <div className="space-y-6 text-[17px] leading-[1.7] text-muted">
                <p>
                  Aura takes familiar object-oriented tools, adds explicit null
                  safety and managed memory, then carries those decisions
                  through a native build pipeline.
                </p>
                <p className="text-fg">
                  The goal is not to hide systems behavior. It is to give
                  everyday service code a smaller, more predictable operational
                  footprint.
                </p>
              </div>
              <Link to="/rfc/000" className="btn-ghost mt-9">
                Read the design principles
                <IconArrowRight size={15} stroke={1.75} aria-hidden />
              </Link>
            </Reveal>
          </div>
        </section>

        {/* Current status */}
        <section className="border-t border-border bg-tint/60 py-20 md:py-24">
          <div className="home-section grid grid-cols-1 items-start gap-12 md:grid-cols-12 md:gap-10">
            <Reveal y={14} className="md:col-span-5">
              <h2 className="font-display text-[32px] leading-[1.12] font-medium tracking-tight md:text-[40px]">
                Useful today.
                <span className="block italic text-muted">
                  Honest about what comes next.
                </span>
              </h2>
              <p className="mt-5 max-w-[420px] text-[16px] leading-[1.6] text-muted">
                Aura is open source and developed against {rfcCount} public
                RFCs, an executable corpus, and repository examples.
              </p>
            </Reveal>

            <motion.div
              className="md:col-span-6 md:col-start-7"
              initial="hidden"
              whileInView="show"
              viewport={{ once: true, amount: 0.2 }}
              variants={{
                hidden: {},
                show: {
                  transition: { staggerChildren: 0.08, delayChildren: 0.06 },
                },
              }}
            >
              <div className="rounded-2xl border border-border bg-card p-7">
                <h3 className="font-display text-[22px] tracking-tight">
                  Works today
                </h3>
                <ul className="mt-5 m-0 list-none space-y-4 p-0">
                  {WORKS_TODAY.map((row) => (
                    <motion.li
                      key={row.t}
                      className="flex gap-3"
                      variants={{
                        hidden: { opacity: 0, y: 10 },
                        show: {
                          opacity: 1,
                          y: 0,
                          transition: { duration: 0.6, ease: easeOutExpo },
                        },
                      }}
                    >
                      <IconCircleCheck
                        size={19}
                        stroke={1.5}
                        className="mt-0.5 shrink-0 text-accent"
                        aria-hidden
                      />
                      <span className="text-[15px] leading-snug text-fg">
                        {row.t}
                      </span>
                    </motion.li>
                  ))}
                </ul>
              </div>

              <motion.div
                className="mt-5 rounded-2xl border border-border bg-bg p-7"
                variants={{
                  hidden: { opacity: 0, y: 10 },
                  show: {
                    opacity: 1,
                    y: 0,
                    transition: { duration: 0.6, ease: easeOutExpo },
                  },
                }}
              >
                <h3 className="font-display text-[22px] tracking-tight">
                  Intentionally later
                </h3>
                <ul className="mt-5 m-0 list-none space-y-3 p-0 text-[15px] leading-snug text-muted">
                  {COMES_LATER.map((item) => (
                    <li key={item} className="flex gap-3">
                      <IconFileText
                        size={18}
                        stroke={1.5}
                        className="mt-0.5 shrink-0 text-ink-muted"
                        aria-hidden
                      />
                      <span>{item}</span>
                    </li>
                  ))}
                </ul>
              </motion.div>
            </motion.div>
          </div>
        </section>

        {/* Final CTA */}
        <section className="border-t border-border py-20 md:py-24">
          <div className="home-section">
            <Reveal y={16}>
              <div className="lift-md rounded-[28px] border border-border-strong bg-card px-8 py-12 text-center md:px-16 md:py-16">
                <h2 className="mx-auto max-w-[640px] font-display text-[32px] leading-[1.12] font-medium tracking-tight text-balance md:text-[44px]">
                  Start with a small program,
                  <span className="italic text-muted">
                    {' '}
                    then inspect every design decision.
                  </span>
                </h2>
                <p className="mx-auto mt-5 max-w-[520px] text-[16px] leading-[1.6] text-muted">
                  The guides teach the language. The RFCs explain the tradeoffs
                  behind it.
                </p>
                <div className="mt-9 flex flex-wrap items-center justify-center gap-4">
                  <Link to="/docs" className="btn-primary">
                    Read the docs
                    <IconArrowRight size={16} stroke={1.75} aria-hidden />
                  </Link>
                  <Link to="/rfc" className="btn-ghost">
                    Browse RFCs
                    <IconArrowRight size={15} stroke={1.75} aria-hidden />
                  </Link>
                  <a
                    href="https://github.com/auraspace/aura"
                    className="btn-ghost"
                    rel="noreferrer"
                    target="_blank"
                  >
                    <IconBrandGithub size={16} stroke={1.75} aria-hidden />
                    GitHub
                    <IconExternalLink size={14} stroke={1.75} aria-hidden />
                  </a>
                </div>
              </div>
            </Reveal>
          </div>
        </section>

        <footer className="border-t border-border py-10">
          <div className="home-section flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
            <p className="font-display text-[16px] font-medium tracking-tight">
              Aura
            </p>
            <p className="text-[13px] text-muted">
              MIT licensed. Designed and built in public.
            </p>
            <nav className="flex flex-wrap gap-5">
              <Link to="/docs" className="navlink">
                Docs
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
                <IconBrandGithub size={15} stroke={1.75} aria-hidden />
                GitHub
              </a>
            </nav>
          </div>
        </footer>
      </div>
    </MotionConfig>
  )
}
