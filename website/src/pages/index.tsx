import React from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import Layout from '@theme/Layout';
import {
  IconBolt,
  IconBrandRust,
  IconBrain,
  IconCode,
  IconRefresh,
  IconTool,
  IconRocket,
  IconBrandGithub,
  IconBook,
  IconArrowRight,
  IconTerminal,
  IconCircleDot,
  IconFileCode,
  IconAbc,
  IconTree,
  IconCheck,
  IconBinaryTree,
  IconCpu,
  IconPlayerPlay,
  IconCopy,
} from '@tabler/icons-react';

import styles from './index.module.css';

/* ─── Syntax-highlighted code lines helper ─────────────────────────────── */
type CProps = {
  children: React.ReactNode;
  type?: 'kw' | 'tp' | 'st' | 'nm' | 'cm' | 'fn' | 'op' | 'varColor';
};

function C({ children, type }: CProps): React.JSX.Element {
  let cls = styles.varColor;
  if (type) {
    cls =
      {
        kw: styles.kwColor,
        tp: styles.typeColor,
        st: styles.strColor,
        nm: styles.numColor,
        cm: styles.cmtColor,
        fn: styles.fnColor,
        op: styles.opColor,
        varColor: styles.varColor,
      }[type] || styles.varColor;
  }

  return <span className={cls}>{children}</span>;
}

/* ─── Code Window ──────────────────────────────────────────────────────── */
type CodeWindowProps = {
  filename: string;
  lines: React.ReactNode[];
};

function CodeWindow({ filename, lines }: CodeWindowProps): React.JSX.Element {
  return (
    <div className={styles.codeWindow}>
      <div className={styles.codeWindowBar}>
        <span className={clsx(styles.codeWindowDot, styles.codeWindowDotRed)} />
        <span
          className={clsx(styles.codeWindowDot, styles.codeWindowDotYellow)}
        />
        <span
          className={clsx(styles.codeWindowDot, styles.codeWindowDotGreen)}
        />
        <span className={styles.codeWindowTitle}>
          <IconFileCode
            size={13}
            style={{ verticalAlign: 'middle', marginRight: 4 }}
          />
          {filename}
        </span>
      </div>
      <div className={styles.codeWindowBody}>
        {lines.map((line, i) => (
          <div key={i} className={styles.codeLine}>
            <span className={styles.codeLineNum}>{i + 1}</span>
            <span className={styles.codeLineContent}>{line}</span>
          </div>
        ))}
        <div className={styles.codeLine}>
          <span className={styles.codeLineNum}>{lines.length + 1}</span>
          <span className={styles.codeLineContent}>
            <span className={styles.typedCursor} />
          </span>
        </div>
      </div>
    </div>
  );
}

/* ─── Hero code lines ───────────────────────────────────────────────────── */
const heroCodeLines: React.ReactNode[] = [
  <React.Fragment key="l1">
    <C type="cm">{'// Aura — Fast, Safe, Expressive'}</C>
  </React.Fragment>,
  <React.Fragment key="l2"></React.Fragment>,
  <React.Fragment key="l3">
    <C type="kw">class </C>
    <C type="tp">Animal</C>
    <C type="op"> {'{'}</C>
  </React.Fragment>,
  <React.Fragment key="l4">
    &nbsp;&nbsp;<C type="kw">public </C>
    <C type="varColor">name</C>
    <C type="op">: </C>
    <C type="tp">string</C>
    <C type="op">;</C>
  </React.Fragment>,
  <React.Fragment key="l5">
    &nbsp;&nbsp;<C type="kw">public </C>
    <C type="varColor">sound</C>
    <C type="op">: </C>
    <C type="tp">string</C>
    <C type="op">;</C>
  </React.Fragment>,
  <React.Fragment key="l6"></React.Fragment>,
  <React.Fragment key="l7">
    &nbsp;&nbsp;<C type="kw">public </C>
    <C type="fn">constructor</C>
    <C type="op">(</C>
    <C type="varColor">name</C>
    <C type="op">: </C>
    <C type="tp">string</C>
    <C type="op">, </C>
    <C type="varColor">sound</C>
    <C type="op">: </C>
    <C type="tp">string</C>
    <C type="op">) {'{'}</C>
  </React.Fragment>,
  <React.Fragment key="l8">
    &nbsp;&nbsp;&nbsp;&nbsp;<C type="kw">this</C>
    <C type="op">.</C>
    <C type="varColor">name</C>
    <C type="op"> = </C>
    <C type="varColor">name</C>
    <C type="op">;</C>
  </React.Fragment>,
  <React.Fragment key="l9">
    &nbsp;&nbsp;&nbsp;&nbsp;<C type="kw">this</C>
    <C type="op">.</C>
    <C type="varColor">sound</C>
    <C type="op"> = </C>
    <C type="varColor">sound</C>
    <C type="op">;</C>
  </React.Fragment>,
  <React.Fragment key="l10">
    &nbsp;&nbsp;<C type="op">{'}'}</C>
  </React.Fragment>,
  <React.Fragment key="l11"></React.Fragment>,
  <React.Fragment key="l12">
    &nbsp;&nbsp;<C type="kw">public </C>
    <C type="fn">speak</C>
    <C type="op">() {'{'}</C>
  </React.Fragment>,
  <React.Fragment key="l13">
    &nbsp;&nbsp;&nbsp;&nbsp;<C type="kw">print </C>
    <C type="st">`</C>
    <C type="st">{'${'}</C>
    <C type="kw">this</C>
    <C type="op">.</C>
    <C type="varColor">name</C>
    <C type="st">
      {'}'} says {'${'}
    </C>
    <C type="kw">this</C>
    <C type="op">.</C>
    <C type="varColor">sound</C>
    <C type="st">{'}'}!`</C>
    <C type="op">;</C>
  </React.Fragment>,
  <React.Fragment key="l14">
    &nbsp;&nbsp;<C type="op">{'}'}</C>
  </React.Fragment>,
  <React.Fragment key="l15">
    <C type="op">{'}'}</C>
  </React.Fragment>,
  <React.Fragment key="l16"></React.Fragment>,
  <React.Fragment key="l17">
    <C type="kw">let </C>
    <C type="varColor">dog</C>
    <C type="op"> = </C>
    <C type="kw">new </C>
    <C type="tp">Animal</C>
    <C type="op">(</C>
    <C type="st">&quot;Rex&quot;</C>
    <C type="op">, </C>
    <C type="st">&quot;Woof&quot;</C>
    <C type="op">);</C>
  </React.Fragment>,
  <React.Fragment key="l18">
    <C type="kw">print </C>
    <C type="varColor">dog</C>
    <C type="op">.</C>
    <C type="fn">speak</C>
    <C type="op">();</C>&nbsp;&nbsp;<C type="cm">{'// Rex says Woof!'}</C>
  </React.Fragment>,
];

/* ─── Features ─────────────────────────────────────────────────────────── */
type FeatureType = {
  Icon: React.ElementType;
  title: string;
  desc: string;
};

const features: FeatureType[] = [
  {
    Icon: IconBolt,
    title: 'Blazing Performance',
    desc: 'Aura compiles directly to native machine code via a custom backend — no VM overhead, no JIT warm-up, just raw AArch64 and x86_64 speed.',
  },
  {
    Icon: IconBrandRust,
    title: 'Rust-Powered Toolchain',
    desc: 'The entire compiler infrastructure is written in Rust, giving you memory safety, fearless concurrency, and zero-cost abstractions throughout.',
  },
  {
    Icon: IconBrain,
    title: 'Smart Type Inference',
    desc: 'A complete static type system with bidirectional inference — catch bugs at compile time without cluttering your code with redundant annotations.',
  },
  {
    Icon: IconCode,
    title: 'Language Server (LSP)',
    desc: 'Built-in LSP support gives you hover info, go-to-definition, completions, and real-time diagnostics in any editor right out of the box.',
  },
  {
    Icon: IconRefresh,
    title: 'Generational GC',
    desc: 'A statically-linked generational garbage collector keeps allocations fast and memory footprint small — all bundled in a single binary.',
  },
  {
    Icon: IconTool,
    title: 'Complete Toolchain',
    desc: 'One ecosystem: compiler, tree-walking interpreter, LSP server, standard library, and integration test suite — all in one repository.',
  },
];

/* ─── Pipeline steps ───────────────────────────────────────────────────── */
type PipelineStep = {
  Icon: React.ElementType;
  name: string;
  desc: string;
};

const pipeline: PipelineStep[] = [
  { Icon: IconFileCode, name: 'Source', desc: '.aura files' },
  { Icon: IconAbc, name: 'Lexer', desc: 'Tokens' },
  { Icon: IconTree, name: 'Parser', desc: 'AST' },
  { Icon: IconCheck, name: 'Sema', desc: 'Types & Scopes' },
  { Icon: IconBinaryTree, name: 'IR', desc: 'SSA Form' },
  { Icon: IconCpu, name: 'Codegen', desc: 'AArch64 / x64' },
  { Icon: IconPlayerPlay, name: 'Binary', desc: 'Native exec' },
];

/* ─── Syntax Comparison ─────────────────────────────────────────────────── */
const auraCodeLines: React.ReactNode[] = [
  <React.Fragment key="l1">
    <C type="kw">function </C>
    <C type="fn">fib</C>
    <C type="op">(</C>
    <C type="varColor">n</C>
    <C type="op">: </C>
    <C type="tp">number</C>
    <C type="op">): </C>
    <C type="tp">number</C>
    <C type="op"> {'{'}</C>
  </React.Fragment>,
  <React.Fragment key="l2">
    &nbsp;&nbsp;<C type="kw">if </C>
    <C type="op">(</C>
    <C type="varColor">n</C>
    <C type="op"> &lt;= </C>
    <C type="nm">1</C>
    <C type="op">) </C>
    <C type="kw">return </C>
    <C type="varColor">n</C>
    <C type="op">;</C>
  </React.Fragment>,
  <React.Fragment key="l3">
    &nbsp;&nbsp;<C type="kw">return </C>
    <C type="fn">fib</C>
    <C type="op">(</C>
    <C type="varColor">n</C>
    <C type="op"> - </C>
    <C type="nm">1</C>
    <C type="op">) + </C>
    <C type="fn">fib</C>
    <C type="op">(</C>
    <C type="varColor">n</C>
    <C type="op"> - </C>
    <C type="nm">2</C>
    <C type="op">);</C>
  </React.Fragment>,
  <React.Fragment key="l4">
    <C type="op">{'}'}</C>
  </React.Fragment>,
  <React.Fragment key="l5"></React.Fragment>,
  <React.Fragment key="l6">
    <C type="kw">print </C>
    <C type="fn">fib</C>
    <C type="op">(</C>
    <C type="nm">10</C>
    <C type="op">);</C>&nbsp;&nbsp;<C type="cm">{'// 55'}</C>
  </React.Fragment>,
];

const tsCodeLines: React.ReactNode[] = [
  <React.Fragment key="l1">
    <C type="kw">function </C>
    <C type="fn">fib</C>
    <C type="op">(</C>
    <C type="varColor">n</C>
    <C type="op">: </C>
    <C type="tp">number</C>
    <C type="op">): </C>
    <C type="tp">number</C>
    <C type="op"> {'{'}</C>
  </React.Fragment>,
  <React.Fragment key="l2">
    &nbsp;&nbsp;<C type="kw">if </C>
    <C type="op">(</C>
    <C type="varColor">n</C>
    <C type="op"> &lt;= </C>
    <C type="nm">1</C>
    <C type="op">) </C>
    <C type="kw">return </C>
    <C type="varColor">n</C>
    <C type="op">;</C>
  </React.Fragment>,
  <React.Fragment key="l3">
    &nbsp;&nbsp;<C type="kw">return </C>
    <C type="fn">fib</C>
    <C type="op">(</C>
    <C type="varColor">n</C>
    <C type="op"> - </C>
    <C type="nm">1</C>
    <C type="op">) + </C>
    <C type="fn">fib</C>
    <C type="op">(</C>
    <C type="varColor">n</C>
    <C type="op"> - </C>
    <C type="nm">2</C>
    <C type="op">);</C>
  </React.Fragment>,
  <React.Fragment key="l4">
    <C type="op">{'}'}</C>
  </React.Fragment>,
  <React.Fragment key="l5"></React.Fragment>,
  <React.Fragment key="l6">
    <C type="varColor">console</C>
    <C type="op">.</C>
    <C type="fn">log</C>
    <C type="op">(</C>
    <C type="fn">fib</C>
    <C type="op">(</C>
    <C type="nm">10</C>
    <C type="op">));</C>&nbsp;<C type="cm">{'// 55'}</C>
  </React.Fragment>,
];

/* ─── Sections ─────────────────────────────────────────────────────────── */
function HeroSection(): React.JSX.Element {
  return (
    <section className={styles.heroBanner}>
      <div className={styles.heroInner}>
        {/* Left */}
        <div className={styles.heroLeft}>
          <div className={styles.heroBadge}>
            <IconCircleDot size={13} className={styles.heroBadgeIcon} />
            Now in active development
          </div>

          <h1 className={styles.heroTitle}>
            The <span className={styles.heroTitleAccent}>Aura</span> Language
          </h1>

          <p className={styles.heroSubtitle}>
            A modern, statically-typed programming language with a Rust-powered
            toolchain — compiled to native code, built for performance and
            clarity.
          </p>

          <div className={styles.heroButtons}>
            <Link className={styles.btnPrimary} to="/docs/intro">
              <IconRocket size={17} />
              Get Started
            </Link>
            <Link
              className={styles.btnSecondary}
              to="https://github.com/auraspace/aura"
            >
              <IconBrandGithub size={17} />
              Star on GitHub
            </Link>
          </div>

          <div className={styles.heroStats}>
            <div className={styles.statItem}>
              <span className={styles.statValue}>Native</span>
              <span className={styles.statLabel}>Code Output</span>
            </div>
            <div className={styles.statItem}>
              <span className={styles.statValue}>Zero</span>
              <span className={styles.statLabel}>Runtime Overhead</span>
            </div>
            <div className={styles.statItem}>
              <span className={styles.statValue}>1 Binary</span>
              <span className={styles.statLabel}>Self-Contained</span>
            </div>
          </div>
        </div>

        {/* Right — code window */}
        <div className={styles.heroRight}>
          <CodeWindow filename="main.aura" lines={heroCodeLines} />
        </div>
      </div>
    </section>
  );
}

function FeaturesSection(): React.JSX.Element {
  return (
    <section className={clsx(styles.section, styles.sectionAlt)}>
      <div className="container">
        <div className={styles.sectionHeader}>
          <span className={styles.sectionEyebrow}>Why Aura</span>
          <h2 className={styles.sectionTitle}>
            Everything you need to build fast
          </h2>
          <p className={styles.sectionSubtitle}>
            Aura is designed from first principles — a language and toolchain
            that prioritizes developer experience, performance, and correctness.
          </p>
        </div>

        <div className={styles.featuresGrid}>
          {features.map(({ Icon, title, desc }) => (
            <div key={title} className={styles.featureCard}>
              <div className={styles.featureIcon}>
                <Icon size={24} stroke={1.75} />
              </div>
              <div className={styles.featureCardTitle}>{title}</div>
              <p className={styles.featureCardDesc}>{desc}</p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

function PipelineSection(): React.JSX.Element {
  return (
    <section className={styles.section}>
      <div className="container">
        <div className={styles.sectionHeader}>
          <span className={styles.sectionEyebrow}>Compiler Architecture</span>
          <h2 className={styles.sectionTitle}>
            From source to binary in one pass
          </h2>
          <p className={styles.sectionSubtitle}>
            Aura&apos;s compiler pipeline is purpose-built for speed. Each stage
            is implemented from scratch in Rust, with no heavy external
            frameworks.
          </p>
        </div>

        <div className={styles.pipelineGrid}>
          {pipeline.map(({ Icon, name, desc }, i) => (
            <React.Fragment key={name}>
              <div className={styles.pipelineStep}>
                <Icon
                  size={28}
                  stroke={1.5}
                  className={styles.pipelineStepIcon}
                />
                <span className={styles.pipelineStepName}>{name}</span>
                <span className={styles.pipelineStepDesc}>{desc}</span>
              </div>
              {i < pipeline.length - 1 && (
                <IconArrowRight
                  size={18}
                  className={styles.pipelineArrowIcon}
                />
              )}
            </React.Fragment>
          ))}
        </div>
      </div>
    </section>
  );
}

function SyntaxSection(): React.JSX.Element {
  return (
    <section className={clsx(styles.section, styles.sectionAlt)}>
      <div className="container">
        <div className={styles.sectionHeader}>
          <span className={styles.sectionEyebrow}>Clean Syntax</span>
          <h2 className={styles.sectionTitle}>Familiar yet refined</h2>
          <p className={styles.sectionSubtitle}>
            Aura feels familiar to TypeScript and Rust developers — with cleaner
            syntax, no semicolons required, and native compilation.
          </p>
        </div>

        <div className={styles.compareGrid}>
          <CodeWindow filename="fibonacci.aura" lines={auraCodeLines} />
          <CodeWindow filename="fibonacci.ts" lines={tsCodeLines} />
        </div>
      </div>
    </section>
  );
}

function CtaSection(): React.JSX.Element {
  return (
    <section className={styles.ctaSection}>
      <div className="container">
        <div className={styles.ctaBox}>
          <h2 className={styles.ctaTitle}>
            Start building with{' '}
            <span className={styles.heroTitleAccent}>Aura</span>
          </h2>
          <p className={styles.ctaSubtitle}>
            Read the docs, browse the source, or clone the repo and compile your
            first Aura program in minutes.
          </p>
          <div className={styles.ctaButtons}>
            <Link className={styles.btnPrimary} to="/docs/intro">
              <IconBook size={17} />
              Read the Docs
            </Link>
            <Link
              className={styles.btnSecondary}
              to="https://github.com/auraspace/aura"
            >
              <IconBrandGithub size={17} />
              View Source
            </Link>
          </div>
          <div className={styles.ctaInstall}>
            <IconTerminal size={15} className={styles.ctaInstallPrompt} />
            <span>
              git clone https://github.com/auraspace/aura.git &amp;&amp; cargo
              build
            </span>
            <IconCopy
              size={14}
              style={{ marginLeft: 'auto', opacity: 0.4, cursor: 'pointer' }}
            />
          </div>
        </div>
      </div>
    </section>
  );
}

/* ─── Default Export ───────────────────────────────────────────────────── */
export default function Home(): React.JSX.Element {
  return (
    <Layout
      title="Aura — A Modern Systems Language"
      description="Aura is a modern, statically-typed language with a Rust-powered compiler toolchain — native code generation, LSP, and a built-in GC."
    >
      <HeroSection />
      <FeaturesSection />
      <PipelineSection />
      <SyntaxSection />
      <CtaSection />
    </Layout>
  );
}
