import {
  IconArrowRight,
  IconBinaryTree2,
  IconBolt,
  IconBrandGithub,
  IconCheck,
  IconCircleCheck,
  IconCode,
  IconCompass,
  IconComponents,
  IconCopy,
  IconCpu,
  IconExternalLink,
  IconFileText,
  IconPackage,
  IconShieldCheck,
  IconStack2,
  IconTerminal2,
} from '@tabler/icons-react'
import { AnimatePresence, motion, MotionConfig } from 'motion/react'
import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'

import { AuraCanvas } from '@/components/home/aura-canvas'
import { Reveal, Stagger, StaggerItem } from '@/components/motion/reveal'
import { ensureHighlighter, highlightCode } from '@/lib/highlight'
import { getAllMeta } from '@/lib/rfc/load-rfcs'

const RELEASE_VERSION = '0.1.1-alpha.5'

// Code Showcase Samples (Verified against the current alpha compiler)
const CODE_SAMPLES = [
  {
    id: 'hello',
    title: '1. Hello World',
    filename: 'main.aura',
    code: `package main

import std.io as Io

// Main entry point of the application
fun main() {
    // Print greeting message to standard output
    Io.println("Hello, Aura World!")

    // Instantiate and start backend service
    val server: Service = Service("aura-backend")
    server.start()
}

/// Service class with primary constructor parameter
class Service(val name: String) {
    fun start() {
        Io.println("Server " + this.name + " listening.")
    }
}`,
    output: `[aura check] OK (12ms)
[aura build] Output: ./dist/main (4.2 MB)
$ ./dist/main
Hello, Aura World!
Server aura-backend listening.`,
    highlights: [
      'Native executable',
      'Zero framework overhead',
      'Fast compilation',
    ],
  },
  {
    id: 'null-safety',
    title: '2. Flow Null Safety',
    filename: 'user_service.aura',
    code: `package service

import std.io as Io

/// Model representing a User with optional email
class User(val name: String, val email: String?) {}

// Flow-sensitive null checking function
fun getContact(user: User): String {
    val email = user.email
    if (email != null) {
        // Smart flow narrowing: email is non-null String here
        return email.uppercase()
    }
    return user.name
}

// Fallback handling using Elvis operator (?:)
fun formatUser(user: User): String {
    val emailStr: String = user.email ?: "no-email@aura.dev"
    return user.name + " <" + emailStr + ">"
}`,
    output: `[aura check] Type safety verified (0 NPEs possible)
[aura check] Flow narrowing: email -> String in true branch`,
    highlights: [
      'Non-null by default',
      'Compile-time verification',
      'Smart flow narrowing',
    ],
  },
  {
    id: 'concurrency',
    title: '3. Structured Concurrency',
    filename: 'worker.aura',
    code: `package worker

import std.io as Io
import std.task as Task

/// Worker job payload model
class Job(val id: Int, val name: String) {}

// Async task function
async fun fetchJob(id: Int): Int {
    return id * 10
}

// Spawning and joining concurrent tasks
fun processJobs(count: Int) {
    // Spawn an explicit async task fiber
    val handle = spawn {
        val result: Int = await fetchJob(1)
        Io.println("Job complete: " + result)
        return
    }
    // Synchronize with deterministic execution boundary
    join(handle)
}`,
    output: `[aura check] Structured concurrency check passed
[aura test] Verified async tasks without race conditions`,
    highlights: [
      'Explicit spawn & join',
      'Cooperative async/await',
      'Deterministic executor',
    ],
  },
  {
    id: 'classes',
    title: '4. Classes & Interfaces',
    filename: 'repository.aura',
    code: `package db

import std.io as Io

/// Generic repository abstraction interface
interface Repository<T> {
    fun findById(id: Int): T?
    fun save(item: T)
}

/// Domain User entity
class User(val id: Int, val name: String) {}

/// Concrete implementation of Repository<User>
class UserRepository(val connStr: String) : Repository<User> {
    fun findById(id: Int): User? {
        if (id > 0) {
            return User(id, "Aura Developer")
        }
        return null
    }

    fun save(item: User) {
        Io.println("Saved user: " + item.name)
    }
}`,
    output: `[aura check] Interface Repository<User> correctly implemented
[aura build] C backend codegen complete: 0 warnings`,
    highlights: [
      'Generic interfaces',
      'Familiar OOP semantics',
      'Clean abstraction',
    ],
  },
]

// Workflow Step Samples
const WORKFLOW_STEPS = [
  {
    id: 'check',
    command: 'aura check ./service',
    title: '1. Instant Type Check',
    subtitle: 'Catch bugs in milliseconds with rich compiler diagnostics.',
    terminalOutput: `[1/1] Checking package service...
  ✔  Syntax parsing complete (1.2ms)
  ✔  Symbol resolution complete (2.4ms)
  ✔  Type check & null-safety validation complete (3.8ms)
  
✨  Checked 14 files in 7.4ms. 0 errors, 0 warnings.`,
  },
  {
    id: 'test',
    command: 'aura test ./service',
    title: '2. Built-in Test Suite',
    subtitle: 'Run package unit tests directly without extra task runners.',
    terminalOutput: `[test] Running package tests in ./service...
  RUN  test_user_registration ... OK (0.4ms)
  RUN  test_null_email_fallback ... OK (0.2ms)
  RUN  test_concurrent_channel_drain ... OK (1.1ms)

✔ Passed: 18 tests | 0 failed | 0 skipped (in 8.2ms)`,
  },
  {
    id: 'build',
    command: 'aura build ./service -o service',
    title: '3. Transpile & C Codegen',
    subtitle: 'High-performance C backend emits optimized native binaries.',
    terminalOutput: `[build] Compiling package service...
  ✔  Generating C code (aura_out.c)
  ✔  Invoking native compiler: gcc -O3 aura_out.c runtime.c
  ✔  Linking runtime & garbage collector

🚀  Created self-contained binary: ./service (4.1 MB)`,
  },
  {
    id: 'deploy',
    command: './service',
    title: '4. Zero-Dependency Deploy',
    subtitle:
      'Run directly on server or minimal Docker container without JVM or VM.',
    terminalOutput: `[service] Starting Aura HTTP Server v${RELEASE_VERSION}
[service] Listening on http://0.0.0.0:8080
[service] Active GC: Managed mark-and-sweep enabled
[service] Deterministic concurrency executor initialized (1 thread)
⚡ Ready for requests. Memory footprint: 12.4 MB`,
  },
]

const PILLARS = [
  {
    title: 'Familiar on Purpose',
    body: 'Classes, interfaces, methods, generics, and value types make service code easy to read and maintain without importing heavy frameworks.',
    Icon: IconComponents,
    tag: 'Ergonomics',
  },
  {
    title: 'Null Safety by Default',
    body: 'Variables are non-null by default. Optional types (T?) are explicit, and flow-sensitive compiler narrowing keeps safe paths direct.',
    Icon: IconShieldCheck,
    tag: 'Safety',
  },
  {
    title: 'Self-Contained Native Binary',
    body: 'Garbage collector and runtime support link directly into the standalone binary. Zero runtime dependencies on the target server.',
    Icon: IconPackage,
    tag: 'Deployment',
  },
  {
    title: 'Explicit Concurrency',
    body: 'Spawn, join, channels, and task cancellation have explicit boundaries. The deterministic single-threaded executor prevents hidden races.',
    Icon: IconBinaryTree2,
    tag: 'Concurrency',
  },
]

const SPECTRUM = [
  {
    language: 'Aura',
    isAura: true,
    safety: 'Flow-Sensitive Null Safety',
    binary: 'Single Native Executable',
    memory: 'Embedded Compact GC',
    learningCurve: 'Immediate (Familiar OOP)',
    deployComplexity: 'Minimal (Zero deps)',
  },
  {
    language: 'Go',
    isAura: false,
    safety: 'Nil Pointer Exceptions possible',
    binary: 'Single Native Binary',
    memory: 'Concurrent GC',
    learningCurve: 'Low',
    deployComplexity: 'Low',
  },
  {
    language: 'Java / Kotlin',
    isAura: false,
    safety: 'Null checks required at runtime',
    binary: 'JAR / Requires JVM',
    memory: 'Heavy JVM GC',
    learningCurve: 'Medium',
    deployComplexity: 'Requires JVM / JRE',
  },
  {
    language: 'Rust',
    isAura: false,
    safety: 'Strict Lifetime & Borrow Checker',
    binary: 'Single Native Binary',
    memory: 'Manual / Lifetimes (No GC)',
    learningCurve: 'High',
    deployComplexity: 'Low',
  },
]

const WORKS_TODAY = [
  'Check, build, run, test, format, and emit C from one Rust CLI.',
  'Build multi-file packages with aura.toml and locked dependencies.',
  'Get human-readable diagnostics or structured JSON for IDE tooling.',
  'Compile real corpus programs and repository examples via C backend.',
]

const COMES_LATER = [
  'LLVM production backend target',
  'General async lowering & multi-threaded scheduling',
  'Hosted package registry & toolchain version manager',
]

export function HomePage() {
  const rfcCount = getAllMeta().length
  const [activeCodeTab, setActiveCodeTab] = useState(0)
  const [activeStep, setActiveStep] = useState(0)
  const [copiedInstall, setCopiedInstall] = useState(false)
  const [highlighterReady, setHighlighterReady] = useState(false)

  useEffect(() => {
    ensureHighlighter().then(() => setHighlighterReady(true))
  }, [])

  const currentSample = CODE_SAMPLES[activeCodeTab]
  const currentStep = WORKFLOW_STEPS[activeStep]
  const highlightedHtml = highlightCode(currentSample.code, 'aura')

  const handleCopyInstall = () => {
    navigator.clipboard.writeText(
      `curl -fsSL https://aura.fadosoft.com/install.sh | AURA_VERSION=${RELEASE_VERSION} bash`,
    )
    setCopiedInstall(true)
    setTimeout(() => setCopiedInstall(false), 2000)
  }

  return (
    <MotionConfig reducedMotion="user">
      <div className="relative flex-1 overflow-hidden">
        {/* Top ambient glow background */}
        <div className="glow-mesh absolute inset-x-0 top-0 h-[600px] pointer-events-none opacity-80" />

        {/* HERO SECTION */}
        <section className="relative pb-16 pt-8 md:pb-24 md:pt-14 overflow-hidden">
          <AuraCanvas />
          <div className="home-section relative z-10">
            <div className="grid grid-cols-1 items-center gap-12 lg:grid-cols-12 lg:gap-10">
              {/* Left Hero Text */}
              <div className="lg:col-span-6">
                <Reveal onMount y={8} delay={0.02}>
                  <div className="inline-flex items-center gap-2 rounded-full border border-border bg-card/80 px-3.5 py-1.5 backdrop-blur">
                    <span className="relative flex h-2 w-2">
                      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-accent opacity-75"></span>
                      <span className="relative inline-flex h-2 w-2 rounded-full bg-accent"></span>
                    </span>
                    <span className="font-mono text-[11px] uppercase tracking-[0.12em] text-muted">
                      Aura v{RELEASE_VERSION} • Compiled Service Language
                    </span>
                  </div>
                </Reveal>

                <h1 className="mt-6 font-display text-[42px] leading-[1.04] font-medium tracking-tight text-balance md:text-[66px]">
                  <Reveal onMount y={16} delay={0.08} className="block">
                    Easy to write.
                  </Reveal>
                  <Reveal
                    onMount
                    y={16}
                    delay={0.14}
                    className="block bg-gradient-to-r from-accent via-accent-deep to-fg bg-clip-text text-transparent italic"
                  >
                    Boring to deploy.
                  </Reveal>
                </h1>

                <Reveal onMount y={12} delay={0.2}>
                  <p className="mt-6 max-w-[540px] text-[17px] leading-[1.6] text-muted md:text-[18px]">
                    Aura combines familiar classes, compile-time null safety,
                    managed memory, and single-file native binaries in one
                    Rust-powered toolchain.
                  </p>
                </Reveal>

                {/* Primary Action Row */}
                <Reveal onMount y={10} delay={0.26}>
                  <div className="mt-8 flex flex-wrap items-center gap-4">
                    <Link
                      to="/docs"
                      className="btn-primary shadow-lg shadow-accent/10"
                    >
                      Get Started
                      <IconArrowRight size={16} stroke={1.75} aria-hidden />
                    </Link>
                    <Link to="/rfc" className="btn-ghost">
                      Browse RFCs ({rfcCount})
                      <IconArrowRight size={15} stroke={1.75} aria-hidden />
                    </Link>
                  </div>
                </Reveal>

                {/* One-line Install Box */}
                <Reveal onMount y={10} delay={0.32}>
                  <div className="mt-8 inline-flex max-w-full items-center gap-3 rounded-2xl border border-border bg-card/90 px-4 py-2.5 shadow-sm backdrop-blur">
                    <span className="font-mono text-[11px] text-accent">$</span>
                    <code className="truncate font-mono text-[12.5px] text-fg">
                      curl -fsSL https://aura.fadosoft.com/install.sh |
                      AURA_VERSION={RELEASE_VERSION} bash
                    </code>
                    <button
                      onClick={handleCopyInstall}
                      className="ml-auto shrink-0 rounded-lg p-1.5 text-muted transition-colors hover:bg-tint hover:text-fg cursor-pointer"
                      title="Copy install command"
                      type="button"
                    >
                      {copiedInstall ? (
                        <IconCheck size={15} className="text-accent" />
                      ) : (
                        <IconCopy size={15} />
                      )}
                    </button>
                  </div>
                </Reveal>
              </div>

              {/* Right Hero Code Playground Widget */}
              <Reveal onMount y={16} delay={0.18} className="lg:col-span-6">
                <div className="relative mx-auto w-full max-w-[560px]">
                  {/* Code Card Terminal Frame */}
                  <div className="terminal-box rounded-2xl border border-border-strong bg-card overflow-hidden">
                    {/* Tab Bar */}
                    <div className="flex items-center justify-between border-b border-border bg-tint/60 px-4 py-2.5">
                      <div className="flex items-center gap-1.5">
                        <span className="h-3 w-3 rounded-full bg-danger/60" />
                        <span className="h-3 w-3 rounded-full bg-status-review-border/60" />
                        <span className="h-3 w-3 rounded-full bg-accent/60" />
                      </div>
                      <div className="flex items-center gap-1 font-mono text-[11px] text-ink-muted">
                        <IconCode size={13} />
                        {currentSample.filename}
                      </div>
                    </div>

                    {/* Interactive Code Selectors */}
                    <div className="flex overflow-x-auto border-b border-border bg-card p-1 custom-scrollbar">
                      {CODE_SAMPLES.map((sample, idx) => (
                        <button
                          key={sample.id}
                          onClick={() => setActiveCodeTab(idx)}
                          className={`flex-1 whitespace-nowrap rounded-lg px-3 py-1.5 font-mono text-[11px] font-medium transition-all cursor-pointer ${
                            activeCodeTab === idx
                              ? 'bg-tint text-accent shadow-xs'
                              : 'text-muted hover:text-fg'
                          }`}
                          type="button"
                        >
                          {sample.title}
                        </button>
                      ))}
                    </div>

                    {/* Code Content Window */}
                    <div className="h-[280px] overflow-y-auto bg-code p-4 font-mono text-[12px] leading-[1.65] custom-scrollbar home-page-code">
                      <AnimatePresence mode="wait">
                        <motion.div
                          key={
                            currentSample.id + (highlighterReady ? '-hl' : '')
                          }
                          initial={{ opacity: 0, y: 4 }}
                          animate={{ opacity: 1, y: 0 }}
                          exit={{ opacity: 0, y: -4 }}
                          transition={{ duration: 0.15 }}
                          className="shiki-wrap m-0 text-fg"
                          dangerouslySetInnerHTML={{ __html: highlightedHtml }}
                        />
                      </AnimatePresence>
                    </div>

                    {/* Console Output Bar */}
                    <div className="h-[95px] overflow-y-auto border-t border-border bg-tint/80 p-3.5 custom-scrollbar">
                      <div className="flex items-center justify-between font-mono text-[11px] text-muted pb-1.5">
                        <span className="inline-flex items-center gap-1 text-accent font-semibold">
                          <IconTerminal2 size={13} /> Compiler Output
                        </span>
                        <span>aura build & run</span>
                      </div>
                      <pre className="m-0 overflow-x-auto font-mono text-[11px] leading-relaxed text-ink-muted whitespace-pre-wrap">
                        {currentSample.output}
                      </pre>
                    </div>

                    {/* Highlights Footer */}
                    <div className="flex flex-wrap gap-2 border-t border-border bg-card p-3">
                      {currentSample.highlights.map((h) => (
                        <span
                          key={h}
                          className="inline-flex items-center gap-1 rounded-full border border-border bg-tint px-2.5 py-1 font-mono text-[10px] text-muted"
                        >
                          <IconCheck size={11} className="text-accent" />
                          {h}
                        </span>
                      ))}
                    </div>
                  </div>
                </div>
              </Reveal>
            </div>
          </div>
        </section>

        {/* PILLARS SECTION */}
        <section
          id="features"
          className="border-t border-border py-20 md:py-24 relative bg-card/40"
        >
          <div className="home-section">
            <Reveal y={12}>
              <div className="text-center max-w-[760px] mx-auto">
                <span className="eyebrow">Language Foundations</span>
                <h2 className="mt-3 font-display text-[34px] leading-[1.1] font-medium tracking-tight text-balance md:text-[48px]">
                  Engineered for real-world backend services,
                  <span className="italic text-muted">
                    {' '}
                    without deploy friction.
                  </span>
                </h2>
                <p className="mt-4 text-[17px] leading-[1.6] text-muted">
                  Aura removes runtime setup headaches while providing language
                  safety features expected in modern engineering.
                </p>
              </div>
            </Reveal>

            <Stagger className="mt-14 grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-4">
              {PILLARS.map((pillar) => (
                <StaggerItem key={pillar.title}>
                  <article className="group relative h-full rounded-2xl border border-border bg-card p-6 transition-all duration-200 hover:-translate-y-1 hover:border-accent/40 hover:shadow-lg hover:shadow-accent/5">
                    <div className="flex items-center justify-between mb-4">
                      <div className="flex h-11 w-11 items-center justify-center rounded-xl border border-border bg-tint text-accent transition-colors group-hover:bg-accent group-hover:text-bg">
                        <pillar.Icon size={22} stroke={1.75} aria-hidden />
                      </div>
                      <span className="font-mono text-[10px] uppercase tracking-wider text-muted px-2 py-0.5 rounded border border-border">
                        {pillar.tag}
                      </span>
                    </div>

                    <h3 className="font-display text-[20px] font-medium tracking-tight text-fg group-hover:text-accent transition-colors">
                      {pillar.title}
                    </h3>
                    <p className="mt-3 text-[14.5px] leading-[1.6] text-muted">
                      {pillar.body}
                    </p>
                  </article>
                </StaggerItem>
              ))}
            </Stagger>
          </div>
        </section>

        {/* INTERACTIVE CLI WORKFLOW VISUALIZER */}
        <section className="border-t border-border py-20 md:py-28 bg-tint/40">
          <div className="home-section">
            <Reveal y={14}>
              <div className="flex flex-col md:flex-row md:items-end justify-between gap-6">
                <div>
                  <span className="eyebrow">Developer Workflow</span>
                  <h2 className="mt-3 max-w-[650px] font-display text-[32px] leading-[1.1] font-medium tracking-tight md:text-[44px]">
                    One unified CLI toolchain
                    <span className="italic text-muted">
                      {' '}
                      from local dev to production.
                    </span>
                  </h2>
                </div>
                <p className="max-w-[420px] text-[15px] leading-[1.6] text-muted">
                  No extra build scripts, webpack configs, or third-party task
                  runners needed.
                </p>
              </div>
            </Reveal>

            {/* Interactive Steps Grid */}
            <div className="mt-12 grid grid-cols-1 lg:grid-cols-12 gap-8 items-start">
              {/* Step Selectors */}
              <div className="lg:col-span-5 space-y-3">
                {WORKFLOW_STEPS.map((step, idx) => (
                  <button
                    key={step.id}
                    onClick={() => setActiveStep(idx)}
                    className={`w-full text-left p-5 rounded-2xl border transition-all cursor-pointer ${
                      activeStep === idx
                        ? 'border-accent bg-card shadow-md'
                        : 'border-border bg-card/50 hover:bg-card hover:border-border-strong'
                    }`}
                    type="button"
                  >
                    <div className="flex items-center justify-between">
                      <code className="font-mono text-[13px] font-medium text-accent">
                        $ {step.command}
                      </code>
                      {activeStep === idx && (
                        <span className="h-2 w-2 rounded-full bg-accent" />
                      )}
                    </div>
                    <h3 className="mt-2 font-display text-[18px] font-medium text-fg">
                      {step.title}
                    </h3>
                    <p className="mt-1 text-[13.5px] text-muted leading-relaxed">
                      {step.subtitle}
                    </p>
                  </button>
                ))}
              </div>

              {/* Live Terminal Output Window */}
              <div className="lg:col-span-7">
                <div className="terminal-box rounded-2xl border border-border-strong bg-[#0f1311] p-6 font-mono text-[13px] text-gray-200 h-[260px] flex flex-col">
                  <div className="flex items-center justify-between pb-4 border-b border-gray-800 text-gray-400 text-[11px] shrink-0">
                    <div className="flex items-center gap-2">
                      <IconTerminal2 size={14} className="text-accent" />
                      <span>terminal — {currentStep.command}</span>
                    </div>
                    <span className="text-accent">status: active</span>
                  </div>

                  <div className="pt-4 space-y-3 overflow-y-auto flex-1 custom-scrollbar">
                    <div className="flex items-center gap-2 text-gray-400">
                      <span className="text-accent">❯</span>{' '}
                      {currentStep.command}
                    </div>

                    <AnimatePresence mode="wait">
                      <motion.pre
                        key={currentStep.id}
                        initial={{ opacity: 0, y: 6 }}
                        animate={{ opacity: 1, y: 0 }}
                        exit={{ opacity: 0, y: -6 }}
                        transition={{ duration: 0.15 }}
                        className="m-0 font-mono text-[12.5px] leading-[1.7] text-gray-300 whitespace-pre-wrap"
                      >
                        {currentStep.terminalOutput}
                      </motion.pre>
                    </AnimatePresence>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* LANGUAGE SPECTRUM COMPARISON */}
        <section className="border-t border-border py-20 md:py-24 bg-card/60">
          <div className="home-section">
            <Reveal y={14}>
              <div className="text-center max-w-[700px] mx-auto">
                <span className="eyebrow">Language Trade-offs</span>
                <h2 className="mt-3 font-display text-[32px] leading-[1.1] font-medium tracking-tight md:text-[44px]">
                  Where Aura fits in the ecosystem
                </h2>
                <p className="mt-4 text-[16px] text-muted">
                  Aura targets the sweet spot: high developer productivity with
                  single-file native binaries and safety guaranteed at
                  compile-time.
                </p>
              </div>
            </Reveal>

            <div className="mt-12 overflow-x-auto rounded-2xl border border-border bg-card custom-scrollbar">
              <table className="w-full text-left border-collapse min-w-[640px]">
                <thead>
                  <tr className="border-b border-border bg-tint/60 text-[12px] font-mono text-muted uppercase tracking-wider">
                    <th className="py-4 px-6">Language</th>
                    <th className="py-4 px-6">Null Safety</th>
                    <th className="py-4 px-6">Binary & Deploy</th>
                    <th className="py-4 px-6">Memory Management</th>
                    <th className="py-4 px-6">Learning Curve</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-border text-[14px]">
                  {SPECTRUM.map((row) => (
                    <tr
                      key={row.language}
                      className={
                        row.isAura
                          ? 'bg-accent/5 font-medium'
                          : 'hover:bg-tint/30'
                      }
                    >
                      <td className="py-4 px-6">
                        <span className="flex items-center gap-2 font-display text-[16px]">
                          {row.language}
                          {row.isAura && (
                            <span className="rounded-full bg-accent/20 px-2 py-0.5 font-mono text-[10px] text-accent">
                              This Project
                            </span>
                          )}
                        </span>
                      </td>
                      <td className="py-4 px-6 text-muted">
                        <span
                          className={
                            row.isAura ? 'text-accent font-semibold' : ''
                          }
                        >
                          {row.safety}
                        </span>
                      </td>
                      <td className="py-4 px-6 text-muted">{row.binary}</td>
                      <td className="py-4 px-6 text-muted">{row.memory}</td>
                      <td className="py-4 px-6 text-muted">
                        {row.learningCurve}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </section>

        {/* ROADMAP & STATUS */}
        <section className="border-t border-border py-20 md:py-24 bg-tint/50">
          <div className="home-section grid grid-cols-1 items-start gap-12 lg:grid-cols-12 lg:gap-10">
            <Reveal y={14} className="lg:col-span-5">
              <span className="eyebrow">Project Status</span>
              <h2 className="mt-3 font-display text-[32px] leading-[1.12] font-medium tracking-tight md:text-[40px]">
                Useful today.
                <span className="block italic text-muted">
                  Transparent about what comes next.
                </span>
              </h2>
              <p className="mt-5 text-[16px] leading-[1.6] text-muted">
                Aura is developed open source against {rfcCount} public RFC
                specifications, an extensive executable corpus, and repository
                examples.
              </p>

              <div className="mt-8 flex flex-col gap-3 font-mono text-[12px] text-muted">
                <div className="flex items-center gap-2">
                  <IconBolt size={16} className="text-accent" />
                  <span>100% Rust-powered compiler CLI</span>
                </div>
                <div className="flex items-center gap-2">
                  <IconCpu size={16} className="text-accent" />
                  <span>Native C codegen backend + runtime</span>
                </div>
                <div className="flex items-center gap-2">
                  <IconStack2 size={16} className="text-accent" />
                  <span>Full test suite & corpus regression checks</span>
                </div>
              </div>
            </Reveal>

            <div className="lg:col-span-7 space-y-6">
              {/* Works Today Card */}
              <div className="rounded-2xl border border-border bg-card p-7 shadow-xs">
                <div className="flex items-center justify-between mb-5">
                  <h3 className="font-display text-[22px] font-medium tracking-tight">
                    Works Today
                  </h3>
                  <span className="inline-flex items-center gap-1 rounded-full bg-accent/15 px-3 py-1 font-mono text-[11px] text-accent">
                    <IconCircleCheck size={14} /> Ready
                  </span>
                </div>
                <ul className="m-0 list-none space-y-3.5 p-0">
                  {WORKS_TODAY.map((item) => (
                    <li key={item} className="flex items-start gap-3">
                      <IconCheck
                        size={18}
                        className="mt-0.5 shrink-0 text-accent"
                        aria-hidden
                      />
                      <span className="text-[15px] leading-snug text-fg">
                        {item}
                      </span>
                    </li>
                  ))}
                </ul>
              </div>

              {/* Intentionally Later Card */}
              <div className="rounded-2xl border border-border bg-card/60 p-7">
                <div className="flex items-center justify-between mb-5">
                  <h3 className="font-display text-[22px] font-medium tracking-tight text-muted">
                    Intentionally Later
                  </h3>
                  <span className="font-mono text-[11px] text-muted">
                    Roadmap
                  </span>
                </div>
                <ul className="m-0 list-none space-y-3 p-0 text-[15px] leading-snug text-muted">
                  {COMES_LATER.map((item) => (
                    <li key={item} className="flex items-center gap-3">
                      <IconFileText
                        size={16}
                        className="shrink-0 text-ink-muted"
                        aria-hidden
                      />
                      <span>{item}</span>
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          </div>
        </section>

        {/* FINAL CTA */}
        <section className="border-t border-border py-20 md:py-24 relative overflow-hidden">
          <div className="glow-mesh-subtle absolute inset-0 pointer-events-none" />
          <div className="home-section relative">
            <Reveal y={16}>
              <div className="lift-md rounded-[28px] border border-border-strong bg-card/90 px-8 py-12 text-center md:px-16 md:py-16 backdrop-blur">
                <div className="inline-flex items-center gap-2 rounded-full border border-border bg-tint px-3 py-1 mb-4">
                  <IconCompass size={14} className="text-accent" />
                  <span className="font-mono text-[11px] uppercase tracking-wider text-muted">
                    Explore Aura Ecosystem
                  </span>
                </div>

                <h2 className="mx-auto max-w-[660px] font-display text-[34px] leading-[1.12] font-medium tracking-tight text-balance md:text-[46px]">
                  Start with a small program,
                  <span className="italic text-muted">
                    {' '}
                    then inspect every design decision.
                  </span>
                </h2>
                <p className="mx-auto mt-5 max-w-[540px] text-[16.5px] leading-[1.6] text-muted">
                  Read documentation guides to learn the language syntax, or
                  inspect RFC specifications to understand architecture design
                  choices.
                </p>

                <div className="mt-9 flex flex-wrap items-center justify-center gap-4">
                  <Link to="/docs" className="btn-primary shadow-md">
                    Read the Docs
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

        {/* FULL-WIDTH GIANT BRANDING LANDMARK */}
        <section className="relative overflow-hidden py-12 md:py-20 select-none border-t border-border/40 bg-tint/15">
          <AuraCanvas />
          <div className="glow-mesh absolute inset-0 opacity-50 pointer-events-none" />
          <div className="w-full text-center px-2 relative z-10 pointer-events-none">
            <span className="block font-display text-[19vw] font-black leading-[0.82] tracking-tighter uppercase bg-gradient-to-b from-fg/45 via-accent-deep/30 to-transparent dark:from-fg/50 dark:via-accent/30 dark:to-transparent bg-clip-text text-transparent opacity-85">
              AURA
            </span>
          </div>
        </section>

        {/* FOOTER */}
        <footer className="border-t border-border py-10 bg-tint/30">
          <div className="home-section flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex items-center gap-3">
              <p className="font-display text-[18px] font-semibold tracking-tight text-fg">
                Aura
              </p>
              <span className="font-mono text-[11px] text-muted border border-border px-2 py-0.5 rounded">
                v{RELEASE_VERSION}
              </span>
            </div>
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
