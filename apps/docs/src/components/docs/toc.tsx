'use client';

import { useEffect, useState } from 'react';

type TocEntry = {
  depth: number;
  title: string;
  url: string;
};

export function DocsToc(props: {
  entries: TocEntry[];
  meta?: { updated?: string; version?: string; readTime?: string; editUrl?: string };
}) {
  const [activeId, setActiveId] = useState<string | null>(null);

  useEffect(() => {
    if (props.entries.length === 0) return;
    const ids = props.entries
      .map((e) => e.url.replace(/^#/, ''))
      .filter((id) => id.length > 0);
    const els = ids
      .map((id) => document.getElementById(id))
      .filter((el): el is HTMLElement => el !== null);
    if (els.length === 0) return;

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((e) => e.isIntersecting)
          .sort(
            (a, b) => a.boundingClientRect.top - b.boundingClientRect.top,
          )[0];
        if (visible) setActiveId(visible.target.id);
      },
      { rootMargin: '-40% 0px -50% 0px' },
    );
    for (const el of els) observer.observe(el);
    return () => observer.disconnect();
  }, [props.entries]);

  const normalized = normalizeDepth(props.entries);

  return (
    <aside className="ad-toc-wrap">
      <div className="ad-toc">
        <h4>
          <span className="n">§</span> on this page
        </h4>
        {normalized.map((entry) => {
          const id = entry.url.replace(/^#/, '');
          const on = activeId === id;
          const isSub = entry.depth >= 3;
          return (
            <a
              key={entry.url}
              href={entry.url}
              className={`${isSub ? 'sub ' : ''}${on ? 'on' : ''}`}
            >
              {entry.title}
            </a>
          );
        })}
        {props.meta && (
          <div className="meta">
            {props.meta.updated && (
              <div>
                <b>Updated</b> {props.meta.updated}
              </div>
            )}
            {props.meta.version && (
              <div>
                <b>Version</b> {props.meta.version}
              </div>
            )}
            {props.meta.readTime && (
              <div>
                <b>Read time</b> {props.meta.readTime}
              </div>
            )}
            {props.meta.editUrl && (
              <div style={{ marginTop: 10 }}>
                <a href={props.meta.editUrl} target="_blank" rel="noreferrer">
                  edit on github ↗
                </a>
              </div>
            )}
          </div>
        )}
      </div>
    </aside>
  );
}

function normalizeDepth(entries: TocEntry[]): TocEntry[] {
  if (entries.length === 0) return entries;
  const minDepth = Math.min(...entries.map((e) => e.depth));
  return entries.map((e) => ({ ...e, depth: e.depth - minDepth + 2 }));
}
