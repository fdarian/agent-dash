'use client';

import { useEffect, useMemo, useState } from 'react';

const SESSIONS_INIT = [
  { group: 'my-project', name: 'refactor-auth', active: true, unread: false, sel: false },
  { group: 'my-project', name: 'fix-payments', active: true, unread: true, sel: true },
  { group: 'my-project', name: 'ci-flake', active: false, unread: false, sel: false },
  { group: 'side-project', name: 'add-docs', active: false, unread: false, sel: false },
  { group: 'side-project', name: 'bench-bench', active: true, unread: false, sel: false },
  { group: 'infra', name: 'terraform-plan', active: true, unread: true, sel: false },
];

type StreamLine = {
  t: 'user' | 'ink' | 'muted' | 'dim' | 'accent' | 'plan';
  txt: string;
  delay?: number;
};

const LINES: StreamLine[] = [
  { t: 'user', txt: '$ claude' },
  { t: 'ink', txt: "I'll audit the auth layer and propose changes.", delay: 350 },
  { t: 'muted', txt: 'Reading src/auth/middleware.ts...', delay: 600 },
  { t: 'muted', txt: 'Reading src/auth/session.ts...', delay: 450 },
  { t: 'muted', txt: 'Reading tests/auth.spec.ts...', delay: 450 },
  { t: 'ink', txt: 'Three changes I would recommend:', delay: 700 },
  { t: 'dim', txt: '1. Move tokens to httpOnly cookies', delay: 250 },
  { t: 'dim', txt: '2. Add CSRF middleware at /api/*', delay: 250 },
  { t: 'dim', txt: '3. Rotate session keys on privilege', delay: 250 },
  { t: 'dim', txt: '   escalation, not on fixed interval.', delay: 250 },
  { t: 'plan', txt: '──── Plan ────', delay: 600 },
];

function GithubIcon(props: { size?: number }) {
  const size = props.size ?? 18;
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden>
      <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
    </svg>
  );
}

function LiveTUI(props: { size?: 'sm' | 'lg' }) {
  const size = props.size ?? 'sm';
  const [step, setStep] = useState(0);
  const [blink, setBlink] = useState(true);
  const [sessions, setSessions] = useState(SESSIONS_INIT);

  useEffect(() => {
    if (step >= LINES.length) return;
    const d = LINES[step]?.delay ?? 400;
    const h = setTimeout(() => setStep((s) => s + 1), d);
    return () => clearTimeout(h);
  }, [step]);

  useEffect(() => {
    const h = setInterval(() => {
      setSessions((cur) => {
        const next = cur.map((s) => ({ ...s }));
        const idx = 2 + Math.floor(Math.random() * (next.length - 2));
        next[idx].unread = !next[idx].unread;
        if (Math.random() > 0.6) next[idx].active = !next[idx].active;
        return next;
      });
    }, 2400);
    return () => clearInterval(h);
  }, []);

  useEffect(() => {
    const h = setInterval(() => setBlink((b) => !b), 520);
    return () => clearInterval(h);
  }, []);

  const groups = useMemo(() => {
    const g: Record<string, typeof sessions> = {};
    for (const s of sessions) {
      if (!g[s.group]) g[s.group] = [];
      g[s.group].push(s);
    }
    return g;
  }, [sessions]);

  const visibleLines = LINES.slice(0, step);
  const showCursor = step >= LINES.length - 1;

  return (
    <div className={`ad-tui ${size === 'lg' ? 'lg' : ''}`}>
      <div className="ad-tui-bar">
        <div className="dots"><i /><i /><i /></div>
        <span className="title">agent-dash · tmux</span>
        <span className="right">6 sessions · 4 active</span>
      </div>
      <div className="ad-tui-body">
        <div className="ad-tui-sidebar">
          {Object.keys(groups).map((gk, gi) => (
            <div key={gk}>
              <div className={`ad-grp ${gi === 0 ? 'first' : ''}`}>▾ {gk}</div>
              {groups[gk].map((s) => (
                <div
                  key={s.name}
                  className={`ad-ses ${s.sel ? 'sel' : ''} ${s.unread ? 'unread' : ''}`}
                >
                  <span className="name">{s.name}</span>
                  <span className={`stat ${s.active ? (s.unread ? 'new' : 'on') : 'off'}`}>
                    {s.active ? '●' : '○'}
                  </span>
                </div>
              ))}
            </div>
          ))}
          <div className="ad-hidden-hr">─── Hidden (1) ───</div>
        </div>
        <div className="ad-tui-preview">
          {visibleLines.map((l, i) => {
            if (l.t === 'plan') return <div key={i} className="ad-sep-plan">{l.txt}</div>;
            const cls = {
              user: 'ad-tok-user',
              ink: 'ad-tok-ink',
              muted: 'ad-tok-muted',
              dim: 'ad-tok-muted',
              accent: 'ad-tok-accent',
            }[l.t];
            return <div key={i} className={cls}>{l.txt}</div>;
          })}
          {showCursor && <span className="ad-cursor" style={{ opacity: blink ? 1 : 0 }} />}
        </div>
      </div>
      <div className="ad-tui-foot">
        <span>
          <span className="on">●</span> 4 Active · 3 Groups · fix-payments · unread 2
        </span>
        <span>
          <kbd>j</kbd><span>/</span><kbd>k</kbd>move{' '}
          <kbd>o</kbd>switch <kbd>v</kbd>copy <kbd>?</kbd>help <kbd>q</kbd>quit
        </span>
      </div>
    </div>
  );
}

function SectionLabel(props: { n: string; children: React.ReactNode }) {
  return (
    <div className="ad-section-label">
      <span className="n">{props.n}</span>
      <span>{props.children}</span>
      <span className="line" />
    </div>
  );
}

function Hero() {
  return (
    <div className="ad-hero">
      <div className="ad-hero-col">
        <span className="ad-eyebrow">
          <span className="dot" />
          v0.9.2 · built in rust
        </span>
        <h1 className="ad-display">
          Every agent,
          <br />
          <span className="muted">on one</span>{' '}
          <span className="accent">quiet pane.</span>
        </h1>
        <p className="ad-lede">
          Agent Dash is a terminal dashboard for the Claude Code sessions already running in
          your tmux server. Discover them, watch them think, switch between them — without
          touching the mouse.
        </p>
        <div className="ad-cta-row">
          <a className="ad-btn primary" href="#install">
            brew install agent-dash <span className="key">⏎</span>
          </a>
          <a
            className="ad-btn ghost"
            href="https://github.com/fdarian/agent-dash"
            target="_blank"
            rel="noreferrer"
          >
            <GithubIcon /> github <span className="key">G</span>
          </a>
        </div>
        <div className="ad-meta-row">
          <span><b>macOS</b> · Linux</span>
          <span><b>tmux ≥ 3.2</b></span>
          <span><b>Apache-2.0</b></span>
        </div>
      </div>
      <div className="ad-hero-col ad-hero-right">
        <div className="ad-hero-right-inner">
          <LiveTUI size="sm" />
        </div>
      </div>
    </div>
  );
}

function SpecSheet() {
  return (
    <section className="ad-spec">
      <div className="ad-spec-title">
        One seat. <em>Many agents.</em> Zero context loss.
      </div>
      <p className="ad-spec-sub">
        Claude Code is great at one task. Running four of them in four tmux windows is a
        tab-switching tax. Agent Dash lays every session on one pane, pipes their output in
        real time, and tells you which one just asked you a question.
      </p>
      <LiveTUI size="lg" />
      <div className="ad-callouts">
        <div className="ad-callout">
          <span className="tag">01 · discovery</span>
          <h3>Finds what&apos;s already running</h3>
          <p>
            Recursive pgrep across your tmux panes surfaces every Claude process. No
            registration, no config file. Start a new session with <code>c</code>.
          </p>
        </div>
        <div className="ad-callout">
          <span className="tag">02 · preview</span>
          <h3>Low-latency, ANSI-faithful</h3>
          <p>
            FIFO pipes stream pane output with a capture-pane fallback. Colors, boxes, and
            braille spinners render the way Claude intended them.
          </p>
        </div>
        <div className="ad-callout">
          <span className="tag">03 · state</span>
          <h3>Remembers where you were</h3>
          <p>
            Read markers, collapsed groups, and hidden sessions persist under{' '}
            <code>~/.config/agent-dash/</code>. Restart and resume.
          </p>
        </div>
      </div>
    </section>
  );
}

function Keys() {
  const rows: Array<{ k: string; l: React.ReactNode; accent?: boolean }> = [
    { k: 'j k', l: <><b>Move</b> through sessions</> },
    { k: 'o', l: <><b>Switch</b> to the tmux pane</>, accent: true },
    { k: 'v', l: <><b>Copy mode</b> with vim motions</> },
    { k: '/', l: <><b>Search</b> preview content</> },
    { k: 'c', l: <><b>Create</b> a new session</> },
    { k: 'h', l: <><b>Hide</b> what&apos;s not your problem</> },
    { k: 'r', l: <><b>Mark read</b> and move on</> },
    { k: '?', l: <><b>Help</b> overlay, filterable</> },
  ];
  return (
    <section className="ad-keys">
      <div className="ad-keys-head">
        <div>
          <div className="ad-keys-title">A keyboard, not a dashboard.</div>
          <div style={{ color: 'var(--ink-2)', fontSize: 14, marginTop: 6 }}>
            Every operation takes one key. No menus, no palette, no Cmd-K. It is a TUI.
          </div>
        </div>
        <div className="ad-keys-sub">
          press <kbd>?</kbd> anytime for the full ref
        </div>
      </div>
      <div className="ad-keygrid">
        {rows.map((r, i) => (
          <div className="ad-kb" key={i}>
            <span className={`k ${r.accent ? 'accent' : ''}`}>{r.k}</span>
            <span className="lbl">{r.l}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

function Signals() {
  return (
    <section className="ad-signals">
      <div className="ad-signal">
        <h4>Latency budget</h4>
        <div className="big">~12<span className="unit">ms</span></div>
        <p>
          Between Claude printing a token and it appearing in the preview pane, over a local
          tmux FIFO. Capture-pane polling kicks in automatically when the FIFO can&apos;t be
          created.
        </p>
      </div>
      <div className="ad-signal">
        <h4>Binary size</h4>
        <div className="big">5.8<span className="unit">MB</span></div>
        <p>
          One statically-linked Rust binary. No Electron shell, no background daemon, no
          telemetry. It starts before your terminal finishes painting.
        </p>
      </div>
      <div className="ad-signal">
        <h4>Dependencies</h4>
        <div className="big">tmux<span className="unit"> + claude</span></div>
        <p>
          That&apos;s the dependency list. If you already use Claude Code and tmux, you
          already have everything Agent Dash needs to run.
        </p>
      </div>
    </section>
  );
}

function Install() {
  const [tab, setTab] = useState<'brew' | 'cargo' | 'src'>('brew');
  return (
    <section className="ad-install" id="install">
      <div className="ad-install-head">
        <h2>Install in a line.</h2>
        <span className="hint">cargo · brew · from source</span>
      </div>
      <div className="ad-tabs">
        <button className={tab === 'brew' ? 'on' : ''} onClick={() => setTab('brew')}>
          homebrew
        </button>
        <button className={tab === 'cargo' ? 'on' : ''} onClick={() => setTab('cargo')}>
          cargo
        </button>
        <button className={tab === 'src' ? 'on' : ''} onClick={() => setTab('src')}>
          source
        </button>
      </div>
      <div className="ad-tab-body">
        {tab === 'brew' && (
          <>
            <div>
              <span className="c">$</span>{' '}
              <span className="x">brew install fdarian/tap/agent-dash</span>
            </div>
            <div>
              <span className="c">$</span> <span className="x">agent-dash</span>
            </div>
            <div className="p">
              # that&apos;s the whole install. it will find your claude sessions and show
              them.
            </div>
          </>
        )}
        {tab === 'cargo' && (
          <>
            <div>
              <span className="c">$</span>{' '}
              <span className="x">cargo install agent-dash</span>
            </div>
            <div>
              <span className="c">$</span> <span className="x">agent-dash</span>
            </div>
            <div className="p"># requires rust 1.76+, builds in ~45s on an M-series mac.</div>
          </>
        )}
        {tab === 'src' && (
          <>
            <div>
              <span className="c">$</span>{' '}
              <span className="x">git clone https://github.com/fdarian/agent-dash.git</span>
            </div>
            <div>
              <span className="c">$</span>{' '}
              <span className="x">cd agent-dash &amp;&amp; cargo build --release</span>
            </div>
            <div>
              <span className="c">$</span>{' '}
              <span className="x">./target/release/agent-dash</span>
            </div>
            <div className="p">
              # useful if you want to hack on the preview pipeline or add a keybind.
            </div>
          </>
        )}
      </div>
      <div className="ad-install-foot">
        <span>→ <a href="/docs/getting-started">getting started</a></span>
        <span>→ <a href="/docs/configuration">configuration</a></span>
        <span>→ <a href="/docs/keybinds">keybinds reference</a></span>
        <span>→ <a href="https://github.com/fdarian/agent-dash">contribute on github</a></span>
      </div>
    </section>
  );
}

function Footer() {
  return (
    <footer className="ad-bottom">
      <div className="lhs">
        <span className="v">agent-dash</span>
        <span>v0.9.2</span>
        <span className="dot">·</span>
        <span>Apache-2.0</span>
      </div>
      <div className="rhs">
        <a href="/docs">docs</a>
        <a href="https://github.com/fdarian/agent-dash/releases">changelog</a>
        <a href="https://github.com/fdarian/agent-dash">github</a>
        <span>MMXXVI</span>
      </div>
    </footer>
  );
}

export function LandingPage() {
  return (
    <div>
      <nav className="ad-nav">
        <div className="ad-nav-inner">
          <div className="ad-brand">
            <div className="ad-brand-mark" />
            agent-dash
          </div>
          <div className="ad-nav-links">
            <a href="/docs">docs</a>
            <a href="#install">install</a>
            <a href="https://github.com/fdarian/agent-dash">
              <GithubIcon />
            </a>
          </div>
        </div>
      </nav>

      <div className="ad-wrap">
        <div className="ad-frame">
          <div className="ad-ticks-l" />
          <div className="ad-ticks-r" />

          <Hero />

          <div className="ad-rule" />
          <SectionLabel n="01">the core loop</SectionLabel>
          <SpecSheet />

          <div className="ad-rule" />
          <SectionLabel n="02">controls</SectionLabel>
          <Keys />

          <div className="ad-rule" />
          <SectionLabel n="03">by the numbers</SectionLabel>
          <Signals />

          <div className="ad-rule" />
          <SectionLabel n="04">get it</SectionLabel>
          <Install />

          <div className="ad-rule" />
        </div>
      </div>

      <Footer />
    </div>
  );
}
