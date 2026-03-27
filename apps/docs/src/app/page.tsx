import Link from 'next/link';
import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import { ThemeToggle } from '@/components/theme-toggle';

export const metadata: Metadata = {
  title: 'Agent Dash — Terminal Dashboard for Claude Sessions',
  description:
    'A keyboard-first TUI for managing and monitoring Claude AI sessions in tmux. Built with Rust.',
};

/* ─── Icons ─── */

function GithubIcon() {
  return (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
    </svg>
  );
}

function ArrowRightIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M6 4l4 4-4 4" />
    </svg>
  );
}

/* ─── Grid structural elements ─── */

/** Diamond marker at a grid line intersection */
function Diamond(props: { className?: string }) {
  return (
    <svg
      width="9"
      height="9"
      viewBox="0 0 9 9"
      className={`absolute z-10 hidden lg:block ${props.className ?? ''}`}
    >
      <rect
        x="1.5"
        y="1.5"
        width="6"
        height="6"
        rx="0.5"
        transform="rotate(45 4.5 4.5)"
        fill="var(--color-sidebar)"
        stroke="var(--color-fd-border)"
        strokeWidth="1"
      />
    </svg>
  );
}

/** Horizontal grid line spanning the frame, with diamonds at each end */
function GridLine() {
  return (
    <div className="relative border-t border-fd-border">
      <Diamond className="top-0 left-0 -translate-x-1/2 -translate-y-1/2" />
      <Diamond className="top-0 right-0 translate-x-1/2 -translate-y-1/2" />
    </div>
  );
}

/* ─── Terminal Mockup (always dark) ─── */

function TerminalMockup() {
  return (
    <div className="relative mx-auto max-w-4xl">
      <div className="rounded-xl border border-fd-border dark:border-[#1e2028] bg-[#0e1013] overflow-hidden">
        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-white/[0.06] bg-white/[0.015]">
          <div className="flex gap-1.5">
            <div className="w-2.5 h-2.5 rounded-full bg-[#ff5c57]/70" />
            <div className="w-2.5 h-2.5 rounded-full bg-[#ffbb2e]/70" />
            <div className="w-2.5 h-2.5 rounded-full bg-[#38c149]/70" />
          </div>
          <span className="text-xs text-white/25 ml-2 font-mono">agent-dash</span>
        </div>
        <div className="grid grid-cols-1 md:grid-cols-[200px_1fr] divide-y md:divide-y-0 md:divide-x divide-white/[0.06] min-h-[280px] font-mono text-[13px] leading-[22px]">
          <div className="hidden md:block p-3 text-white/40 select-none">
            <div className="text-white/70">&#x25b8; my-project</div>
            <div className="pl-4 flex justify-between items-center">
              <span className="text-white/40">refactor-auth</span>
              <span className="text-emerald-400 text-[10px]">&#x25cf;</span>
            </div>
            <div className="pl-4 flex justify-between items-center rounded bg-white/[0.06] -mx-1 px-1">
              <span className="text-white/70">fix-payments</span>
              <span className="text-emerald-400 text-[10px]">&#x25cf;</span>
            </div>
            <div className="text-white/70 mt-3">&#x25b8; side-project</div>
            <div className="pl-4 flex justify-between items-center">
              <span className="text-white/40">add-docs</span>
              <span className="text-white/15 text-[10px]">&#x25cb;</span>
            </div>
            <div className="mt-5 text-center text-white/15 text-[11px]">&#x2500;&#x2500;&#x2500; Hidden (1) &#x2500;&#x2500;&#x2500;</div>
          </div>
          <div className="p-3 text-white/50 select-none">
            <div className="text-blue-400/80">$ claude</div>
            <div className="mt-3">I&apos;ll analyze the authentication module and suggest improvements.</div>
            <div className="mt-3 text-white/25">Reading src/auth/middleware.ts...</div>
            <div className="text-white/25">Reading src/auth/session.ts...</div>
            <div className="mt-3">Here&apos;s my analysis:</div>
            <div className="mt-1.5 text-white/40">1. Session tokens should use httpOnly</div>
            <div className="text-white/40 pl-4">cookies instead of localStorage</div>
            <div className="text-white/40">2. Add CSRF protection middleware</div>
            <div className="text-white/40">3. Implement token rotation</div>
            <span className="inline-block w-[7px] h-[15px] bg-white/50 animate-blink ml-0.5 translate-y-[2px]" />
            <div className="mt-4 text-center text-brand/60 text-[11px]">&#x2500;&#x2500;&#x2500;&#x2500; Plan &#x2500;&#x2500;&#x2500;&#x2500;</div>
          </div>
        </div>
        <div className="flex justify-between px-4 py-1.5 border-t border-white/[0.06] bg-white/[0.015] font-mono text-[11px] text-white/25 select-none">
          <span><span className="text-emerald-400">&#x25cf;</span> 3 Active &nbsp;&nbsp;2 Groups</span>
          <span>?:Help &nbsp; v:Copy &nbsp; q:Quit</span>
        </div>
      </div>
    </div>
  );
}

/* ─── Feature Card ─── */

type FeatureCardProps = { title: string; description: string };

function FeatureCard(props: FeatureCardProps) {
  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 transition-colors hover:bg-fd-accent">
      <h3 className="text-[15px] font-semibold tracking-tight text-fd-card-foreground">{props.title}</h3>
      <p className="mt-2 text-sm leading-relaxed text-fd-muted-foreground">{props.description}</p>
    </div>
  );
}

type KeyProps = { children: ReactNode };

function Key(props: KeyProps) {
  return (
    <kbd className="inline-flex items-center justify-center min-w-[28px] h-7 px-1.5 rounded-md border border-fd-border bg-fd-muted text-xs font-mono text-fd-muted-foreground">
      {props.children}
    </kbd>
  );
}

/* ─── Data ─── */

const features: FeatureCardProps[] = [
  { title: 'Live Monitoring', description: 'Real-time preview of Claude sessions with full ANSI rendering. FIFO pipe monitoring for low-latency updates.' },
  { title: 'Vim-like Copy Mode', description: 'Navigate, search, and yank text with familiar vim motions. Visual selection, forward/backward search, clipboard support.' },
  { title: 'Session Grouping', description: "Sessions organized by tmux session name with collapsible groups. Hide sessions you don't need, toggle flat view." },
  { title: 'Persistent State', description: 'Read markers, visibility, and collapse state saved across restarts. Pick up right where you left off.' },
  { title: 'Keyboard-First', description: 'Every action accessible via intuitive keybinds. Built-in help overlay with searchable reference.' },
  { title: 'Configurable Layout', description: 'Vertical or horizontal pane arrangement. Maximize or minimize the session list. Custom name formatting.' },
];

const keybinds = [
  { keys: ['j', 'k'], label: 'Navigate' },
  { keys: ['o'], label: 'Switch pane' },
  { keys: ['v'], label: 'Copy mode' },
  { keys: ['h'], label: 'Hide' },
  { keys: ['/'], label: 'Search' },
  { keys: ['?'], label: 'Help' },
];

/* ─── Page ─── */

export default function HomePage() {
  return (
    <div className="min-h-screen bg-fd-background text-fd-foreground antialiased">
      {/* ─── Nav ─── */}
      <nav className="sticky top-0 z-50 border-b border-fd-border bg-fd-background/80 backdrop-blur-xl">
        <div className="mx-auto max-w-6xl px-8 h-14 flex items-center justify-between">
          <Link href="/" className="text-[15px] font-bold tracking-tight">Agent Dash</Link>
          <div className="flex items-center gap-6">
            <Link href="/docs" className="text-sm text-fd-muted-foreground hover:text-fd-foreground transition-colors">Docs</Link>
            <ThemeToggle />
            <a href="https://github.com/fdarian/agent-dash" className="text-fd-muted-foreground hover:text-fd-foreground transition-colors" target="_blank" rel="noopener noreferrer"><GithubIcon /></a>
          </div>
        </div>
      </nav>

      {/* ─── Grid frame: structural borders framing all content ─── */}
      <div className="mx-4 sm:mx-6 lg:mx-auto max-w-6xl relative border-l border-r border-fd-border">
        {/* Ruler tick marks along left and right edges */}
        <div className="ruler-ticks-r absolute top-0 bottom-0 left-0 w-[5px] pointer-events-none opacity-60 hidden lg:block" />
        <div className="ruler-ticks-l absolute top-0 bottom-0 right-0 w-[5px] pointer-events-none opacity-60 hidden lg:block" />

        {/* Dashed vertical center line */}
        <div className="absolute top-0 bottom-0 left-1/2 w-px border-l border-dashed border-fd-border/40 pointer-events-none hidden lg:block" />

        {/* ═══ Hero ═══ */}
        <section className="pt-28 pb-16 px-8 relative overflow-hidden">
          {/* Micro-grid fading in from bottom — subtle orange tint */}
          <div
            className="absolute inset-0 pointer-events-none"
            style={{
              backgroundImage:
                'linear-gradient(var(--color-brand) 1px, transparent 1px), linear-gradient(90deg, var(--color-brand) 1px, transparent 1px)',
              backgroundSize: '24px 24px',
              opacity: 0.03,
              maskImage: 'linear-gradient(to bottom, transparent 40%, black 100%)',
              WebkitMaskImage: 'linear-gradient(to bottom, transparent 40%, black 100%)',
            }}
          />
          <div className="mx-auto max-w-3xl text-center">
            <div className="animate-fade-up inline-flex items-center gap-2 rounded-full border border-fd-border bg-fd-card px-4 py-1.5 text-sm text-fd-muted-foreground mb-8">
              Open Source &middot; Built with Rust
            </div>
            <h1 className="animate-fade-up text-5xl sm:text-6xl lg:text-[4.5rem] font-bold tracking-[-0.03em] leading-[1.05]" style={{ animationDelay: '80ms' }}>
              Your Claude sessions,<br />at a glance
            </h1>
            <p className="animate-fade-up mt-6 text-lg text-fd-muted-foreground max-w-xl mx-auto leading-relaxed" style={{ animationDelay: '160ms' }}>
              A keyboard-first terminal dashboard for managing and monitoring Claude AI sessions in tmux.
            </p>
            <div className="animate-fade-up flex flex-wrap gap-3 justify-center mt-10" style={{ animationDelay: '240ms' }}>
              <Link href="/docs" className="fancy-button inline-flex items-center gap-2 rounded-[10px] bg-brand px-5 py-2.5 text-sm font-medium text-white shadow-[inset_0_1px_0_rgba(255,255,255,0.12),0_1px_3px_rgba(0,0,0,0.2),0_2px_8px_rgba(217,119,87,0.25)] hover:brightness-110 transition">
                Get Started <ArrowRightIcon />
              </Link>
              <a href="https://github.com/fdarian/agent-dash" className="inline-flex items-center gap-2 rounded-[10px] border border-fd-border bg-fd-card px-5 py-2.5 text-sm font-medium text-fd-foreground hover:bg-fd-accent transition" target="_blank" rel="noopener noreferrer">
                <GithubIcon /> View Source
              </a>
            </div>
          </div>
        </section>

        {/* ═══ Terminal Demo ═══ */}
        <GridLine />
        <section className="animate-fade-up px-8 py-10" style={{ animationDelay: '400ms' }}>
          <TerminalMockup />
        </section>

        {/* ═══ Video Placeholder ═══ */}
        <section className="px-8 pb-10">
          <div className="mx-auto max-w-4xl">
            <div className="relative aspect-video rounded-xl border border-dashed border-fd-border bg-fd-card overflow-hidden group cursor-pointer hover:border-fd-muted-foreground/30 transition-colors">
              <div className="absolute inset-0 flex flex-col items-center justify-center gap-3">
                <div className="w-14 h-14 rounded-full border border-fd-border bg-fd-muted flex items-center justify-center group-hover:bg-fd-accent transition-colors">
                  <svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor" className="ml-0.5 text-fd-muted-foreground"><polygon points="4,2 18,10 4,18" /></svg>
                </div>
                <span className="text-sm text-fd-muted-foreground">Demo video placeholder</span>
              </div>
            </div>
          </div>
        </section>

        {/* ═══ Features ═══ */}
        <GridLine />
        <section className="px-8 py-24">
          <div className="text-center mb-14">
            <h2 className="text-3xl sm:text-4xl font-bold tracking-[-0.02em]">Built for power users</h2>
            <p className="mt-4 text-fd-muted-foreground text-lg">Everything you need to manage Claude sessions, nothing you don&apos;t.</p>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {features.map((f) => <FeatureCard key={f.title} title={f.title} description={f.description} />)}
          </div>
        </section>

        {/* ═══ Keybinds ═══ */}
        <GridLine />
        <section className="px-8 py-24">
          <div className="mx-auto max-w-3xl text-center">
            <h2 className="text-3xl sm:text-4xl font-bold tracking-[-0.02em]">Keyboard-first design</h2>
            <p className="mt-4 text-fd-muted-foreground text-lg mb-12">Every action at your fingertips. Press <Key>?</Key> for the full reference.</p>
            <div className="inline-grid grid-cols-2 sm:grid-cols-3 gap-x-10 gap-y-5 text-left">
              {keybinds.map((b) => (
                <div key={b.label} className="flex items-center gap-3">
                  <span className="flex gap-1">{b.keys.map((k) => <Key key={k}>{k}</Key>)}</span>
                  <span className="text-sm text-fd-muted-foreground">{b.label}</span>
                </div>
              ))}
            </div>
          </div>
        </section>

        {/* ═══ Quick Start ═══ */}
        <GridLine />
        <section className="px-8 py-24">
          <div className="mx-auto max-w-2xl text-center">
            <h2 className="text-3xl sm:text-4xl font-bold tracking-[-0.02em] mb-4">Get started in seconds</h2>
            <p className="text-fd-muted-foreground text-lg mb-10">Clone, build, run.</p>
            <div className="rounded-xl border border-fd-border dark:border-[#1e2028] bg-[#0e1013] p-5 text-left font-mono text-[13px] leading-7 overflow-x-auto">
              <div className="text-white/35"><span className="text-emerald-400/70">$</span> git clone https://github.com/fdarian/agent-dash.git</div>
              <div className="text-white/35"><span className="text-emerald-400/70">$</span> cd agent-dash &amp;&amp; cargo build --release</div>
              <div className="text-white/35"><span className="text-emerald-400/70">$</span> ./target/release/agent-dash</div>
            </div>
            <Link href="/docs/installation" className="inline-flex items-center gap-1 mt-6 text-sm text-brand hover:brightness-125 transition">
              Full installation guide <ArrowRightIcon />
            </Link>
          </div>
        </section>

        {/* Bottom frame line */}
        <GridLine />
      </div>

      {/* ─── Footer ─── */}
      <footer>
        <div className="mx-auto max-w-6xl px-8 py-12">
          <div className="flex flex-col sm:flex-row justify-between items-center gap-6">
            <div className="text-sm text-fd-muted-foreground">Agent Dash &mdash; Open source, Apache-2.0 licensed.</div>
            <div className="flex items-center gap-8">
              <Link href="/docs" className="text-sm text-fd-muted-foreground hover:text-fd-foreground transition-colors">Documentation</Link>
              <a href="https://github.com/fdarian/agent-dash" className="text-sm text-fd-muted-foreground hover:text-fd-foreground transition-colors" target="_blank" rel="noopener noreferrer">GitHub</a>
            </div>
          </div>
        </div>
      </footer>
    </div>
  );
}
