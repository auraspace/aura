import {
  IconArrowRight,
  IconBinaryTree2,
  IconBrandGithub,
  IconCheck,
  IconCircleCheck,
  IconCode,
  IconComponents,
  IconExternalLink,
  IconFileText,
  IconPackage,
  IconPlayerPlay,
  IconRocket,
  IconShieldCheck,
  IconTerminal2,
  type Icon,
} from '@tabler/icons-react'
import { MotionConfig, motion } from 'motion/react'
import { Link } from 'react-router-dom'
import {
  Reveal,
  Stagger,
  StaggerItem,
  easeOutExpo,
} from '@/components/motion/reveal'
import { getAllMeta } from '@/lib/rfc/load-rfcs'

const FEATURES: {
  n: string
  title: string
  body: string
  Icon: Icon
}[] = [
  {
    n: '01',
    title: 'One artifact from source',
    body: 'aura build produces a single native executable. The GC and scheduler link into the binary — no runtime install on the host.',
    Icon: IconPackage,
  },
  {
    n: '02',
    title: 'Null-safe by default',
    body: 'T is non-null. T? is opt-in. Flow-sensitive narrowing keeps the safe path short and the escape hatch explicit.',
    Icon: IconShieldCheck,
  },
  {
    n: '03',
    title: 'Tasks, not thread soup',
    body: 'Go-like M:N lightweight tasks and channels for concurrent I/O. Familiar class model without ownership ceremony.',
    Icon: IconBinaryTree2,
  },
  {
    n: '04',
    title: 'Classes and value types',
    body: 'Java-like classes and interfaces for the domain. Distinct structs when you want values, not references.',
    Icon: IconComponents,
  },
  {
    n: '05',
    title: 'Toolchain is the language',
    body: 'check, build, run, test, and packages are first-class CLI verbs — not a pile of third-party scripts.',
    Icon: IconTerminal2,
  },
  {
    n: '06',
    title: 'Designed in public RFCs',
    body: 'Vision, types, memory, runtime, and packages are written down before they ossify. Read the decisions, not just the code.',
    Icon: IconFileText,
  },
]

const METHOD: {
  n: string
  title: string
  body: string
  Icon: Icon
}[] = [
  {
    n: '01',
    title: 'Write what you already know.',
    body: 'Classes, methods, interfaces, and a statement-oriented surface. Hello world needs no framework and no ceremony.',
    Icon: IconCode,
  },
  {
    n: '02',
    title: 'Let the compiler hold the line.',
    body: 'Nullability, exhaustiveness, and package boundaries surface early. Diagnostics are part of the product, not an afterthought.',
    Icon: IconShieldCheck,
  },
  {
    n: '03',
    title: 'Ship one file.',
    body: 'The default deploy story is a single executable you can copy onto a server, drop in a container, or hand out as a CLI.',
    Icon: IconRocket,
  },
]

const PROOF = [
  {
    n: '01',
    t: 'Nullability and Result live in the type system, not style guides.',
  },
  {
    n: '02',
    t: 'Single-binary deploy is a design principle, not a packaging tip.',
  },
  {
    n: '03',
    t: 'Corpus programs compile and run through the aura CLI today.',
  },
] as const

function HeroCodeCard() {
  return (
    <div className="float-y relative mx-auto w-full max-w-[380px] md:ml-auto md:mr-0">
      <div className="lift-md relative rounded-[28px] border border-border-strong bg-card p-5">
        <div className="flex items-center justify-between pb-4">
          <div className="flex items-center gap-1.5">
            <span className="inline-block h-1.5 w-1.5 rounded-full bg-accent" />
            <span className="eyebrow">Compile</span>
          </div>
          <span className="eyebrow inline-flex items-center gap-1 text-ink-muted">
            <IconTerminal2 size={12} stroke={1.75} aria-hidden />
            hello.aura
          </span>
        </div>

        <div className="overflow-hidden rounded-2xl border border-border bg-tint p-4 font-mono text-[12.5px] leading-[1.65]">
          <div className="text-muted">{'// C0 corpus'}</div>
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
              one binary
            </span>
            <span className="eyebrow text-ink-muted">native</span>
          </div>
          <div className="mt-1 flex items-center justify-between gap-2">
            <span className="font-mono text-[11px] text-muted">
              aura run hello.aura
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
                <div className="inline-flex items-center gap-2 rounded-full border border-border-strong bg-card px-3 py-1.5">
                  <span className="inline-block h-1.5 w-1.5 rounded-full bg-accent" />
                  <span className="eyebrow">Open source · Rust toolchain</span>
                </div>
              </Reveal>

              <h1 className="mt-7 font-display text-[40px] leading-[1.05] font-medium tracking-tight text-balance md:text-[68px] md:leading-[1.02]">
                <Reveal onMount y={16} delay={0.08} className="block">
                  Write services that
                </Reveal>
                <Reveal
                  onMount
                  y={16}
                  delay={0.14}
                  className="block italic text-muted"
                >
                  leave as one binary.
                </Reveal>
              </h1>

              <Reveal onMount y={12} delay={0.2}>
                <p className="mt-7 max-w-[520px] text-pretty text-[17px] leading-[1.55] text-muted md:text-[18px]">
                  Aura is a statically typed language with Java-like classes,
                  null-safe types, and Go-like tasks. The runtime ships inside a
                  single native executable.
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

              <Reveal onMount y={8} delay={0.32}>
                <p className="mt-5 eyebrow text-ink-muted">
                  MIT · {rfcCount} RFCs · compiler through C5n
                </p>
              </Reveal>
            </div>

            <Reveal onMount y={16} delay={0.18} className="md:col-span-5">
              <HeroCodeCard />
            </Reveal>
          </div>
        </section>

        {/* Features */}
        <section id="features" className="border-t border-border py-20 md:py-24">
          <div className="home-section">
            <Reveal y={12}>
              <p className="eyebrow">What you get</p>
              <h2 className="mt-4 max-w-[720px] font-display text-[34px] leading-[1.1] font-medium tracking-tight text-balance md:text-[48px]">
                Small promises.
                <span className="italic text-muted">
                  {' '}
                  Kept all the way to the binary.
                </span>
              </h2>
            </Reveal>

            <Stagger className="mt-14 grid grid-cols-1 gap-x-10 gap-y-12 sm:grid-cols-2 lg:grid-cols-3">
              {FEATURES.map((f) => (
                <StaggerItem key={f.n} className="max-w-[360px]">
                  <article>
                    <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-full border border-border bg-card text-accent">
                      <f.Icon size={20} stroke={1.5} aria-hidden />
                    </div>
                    <span className="eyebrow text-ink-muted">{f.n}</span>
                    <h3 className="mt-3 font-display text-[22px] leading-snug tracking-tight">
                      {f.title}
                    </h3>
                    <p className="mt-3 text-[15px] leading-[1.55] text-muted">
                      {f.body}
                    </p>
                  </article>
                </StaggerItem>
              ))}
            </Stagger>
          </div>
        </section>

        {/* Story */}
        <section className="border-t border-border bg-tint/60 py-20 md:py-28">
          <div className="home-section grid grid-cols-1 gap-12 md:grid-cols-12 md:gap-10">
            <Reveal
              y={14}
              className="md:col-span-4 md:sticky md:top-28 md:self-start"
            >
              <aside>
                <p className="eyebrow">Origin</p>
                <h2 className="mt-4 font-display text-[32px] leading-[1.1] font-medium tracking-tight md:text-[36px]">
                  Why Aura
                  <span className="block italic text-muted">exists.</span>
                </h2>
                <p className="mt-5 max-w-[280px] text-[15px] leading-[1.55] text-muted">
                  A middle path between everyday productivity and a deploy story
                  that stays simple.
                </p>
                <dl className="mt-8 space-y-3 border-t border-border pt-6 text-[13px]">
                  <div className="flex justify-between gap-4">
                    <dt className="eyebrow text-ink-muted">Spec</dt>
                    <dd className="font-medium text-fg">RFC-000</dd>
                  </div>
                  <div className="flex justify-between gap-4">
                    <dt className="eyebrow text-ink-muted">Status</dt>
                    <dd className="font-medium text-fg">Accepted</dd>
                  </div>
                  <div className="flex justify-between gap-4">
                    <dt className="eyebrow text-ink-muted">Layer</dt>
                    <dd className="font-medium text-fg">Foundation</dd>
                  </div>
                </dl>
              </aside>
            </Reveal>

            <div className="md:col-span-7 md:col-start-6">
              <Reveal y={16}>
                <blockquote className="font-display text-[28px] leading-[1.25] font-medium tracking-tight text-balance md:text-[36px]">
                  <span className="block">We kept choosing between</span>
                  <span className="block italic text-muted">
                    comfort and a clean deploy.
                  </span>
                </blockquote>
              </Reveal>

              <Stagger
                className="mt-10 space-y-5 text-[17px] leading-[1.7] text-muted"
                staggerDelay={0.06}
              >
                <StaggerItem y={10}>
                  <p>
                    Dynamic runtimes iterate fast, then leave you with two
                    languages: the one you write and the one ops has to install.
                    Systems languages are sharp and safe, but ownership ceremony
                    is heavy for everyday service code.
                  </p>
                </StaggerItem>
                <StaggerItem y={10}>
                  <p>
                    Managed platforms are productive until the footprint and the
                    ship story get in the way. Transpiled stacks bring libraries
                    — and a second runtime gap.
                  </p>
                </StaggerItem>
                <StaggerItem y={10}>
                  <p className="text-fg">
                    Aura aims at the middle path: Java-like productivity,
                    Go-like concurrency and GC, and a single native artifact you
                    can copy onto a machine.
                  </p>
                </StaggerItem>
                <StaggerItem y={10}>
                  <p>
                    The language is Aura. The toolchain is Rust. The long path is
                    LLVM; today a C backend already checks, builds, runs, and
                    tests real packages from this repository.
                  </p>
                </StaggerItem>
              </Stagger>

              <Reveal y={10} delay={0.1} className="mt-10">
                <Link to="/rfc/000" className="btn-ghost">
                  Read RFC-000 · Vision & design principles
                  <IconArrowRight size={15} stroke={1.75} aria-hidden />
                </Link>
              </Reveal>
            </div>
          </div>
        </section>

        {/* Method */}
        <section className="border-t border-border py-20 md:py-24">
          <div className="home-section">
            <Reveal y={12}>
              <p className="eyebrow">The Aura method</p>
              <h2 className="mt-4 max-w-[640px] font-display text-[34px] leading-[1.1] font-medium tracking-tight text-balance md:text-[44px]">
                From familiar source
                <span className="italic text-muted">
                  {' '}
                  to one deployable file.
                </span>
              </h2>
            </Reveal>

            <Stagger
              className="mt-14 grid grid-cols-1 gap-10 md:grid-cols-3 md:gap-8"
              staggerDelay={0.1}
            >
              {METHOD.map((step) => (
                <StaggerItem key={step.n}>
                  <article className="rounded-2xl border border-border bg-card p-6 transition-shadow duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] hover:shadow-[var(--lift)]">
                    <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-full border border-border bg-tint text-accent">
                      <step.Icon size={20} stroke={1.5} aria-hidden />
                    </div>
                    <span className="eyebrow text-ink-muted">{step.n}</span>
                    <h3 className="mt-3 font-display text-[22px] leading-snug tracking-tight">
                      {step.title}
                    </h3>
                    <p className="mt-3 text-[15px] leading-[1.55] text-muted">
                      {step.body}
                    </p>
                  </article>
                </StaggerItem>
              ))}
            </Stagger>
          </div>
        </section>

        {/* Quiet proof */}
        <section className="border-t border-border py-20 md:py-24">
          <div className="home-section grid grid-cols-1 items-start gap-12 md:grid-cols-12">
            <Reveal y={14} className="md:col-span-5">
              <p className="eyebrow">Quiet proof</p>
              <h2 className="mt-4 font-display text-[32px] leading-[1.12] font-medium tracking-tight md:text-[40px]">
                Spec first.
                <span className="block italic text-muted">Then the compiler.</span>
              </h2>
              <p className="mt-5 max-w-[420px] text-[16px] leading-[1.6] text-muted">
                The site you are on indexes the RFCs that lock the language,
                runtime, packages, and CLI — before features silently diverge.
              </p>
            </Reveal>

            <motion.ul
              className="md:col-span-6 md:col-start-7 m-0 list-none space-y-0 divide-y divide-border border-y border-border p-0"
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
              {PROOF.map((row) => (
                <motion.li
                  key={row.n}
                  className="flex gap-4 py-5"
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
                    size={20}
                    stroke={1.5}
                    className="mt-0.5 shrink-0 text-accent"
                    aria-hidden
                  />
                  <span className="text-[16px] leading-snug text-fg">{row.t}</span>
                </motion.li>
              ))}
            </motion.ul>
          </div>
        </section>

        {/* Final CTA */}
        <section className="border-t border-border py-20 md:py-24">
          <div className="home-section">
            <Reveal y={16}>
              <div className="lift-md rounded-[28px] border border-border-strong bg-card px-8 py-12 text-center md:px-16 md:py-16">
                <p className="eyebrow">Start here</p>
                <h2 className="mx-auto mt-4 max-w-[640px] font-display text-[32px] leading-[1.12] font-medium tracking-tight text-balance md:text-[44px]">
                  Learn with the guides,
                  <span className="italic text-muted">
                    {' '}
                    design with the RFCs.
                  </span>
                </h2>
                <div className="mt-9 flex flex-wrap items-center justify-center gap-4">
                  <Link to="/docs" className="btn-primary">
                    Open the docs
                    <IconArrowRight size={16} stroke={1.75} aria-hidden />
                  </Link>
                  <Link to="/rfc" className="btn-ghost">
                    RFC catalog
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
              MIT license · Spec-driven language & toolchain
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
