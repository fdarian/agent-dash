import { source } from '@/lib/source';
import { notFound } from 'next/navigation';
import { getMDXComponents } from '@/components/mdx';
import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import { createRelativeLink } from 'fumadocs-ui/mdx';
import { DocsSidebar } from '@/components/docs/sidebar';
import { DocsToc } from '@/components/docs/toc';
import { getCrumbs, getPagerNeighbors } from '@/components/docs/nav-data';
import Link from 'next/link';

function renderTitle(title: string) {
  const dash = title.indexOf(' — ');
  if (dash < 0) return title;
  return (
    <>
      {title.slice(0, dash)} <span className="accent">{title.slice(dash + 1)}</span>
    </>
  );
}

function toText(node: ReactNode): string {
  if (node === null || node === undefined || typeof node === 'boolean') return '';
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(toText).join('');
  if (typeof node === 'object' && 'props' in node) {
    const el = node as { props: { children?: ReactNode } };
    return toText(el.props.children);
  }
  return '';
}

export default async function Page(props: {
  params: Promise<{ slug?: string[] }>;
}) {
  const params = await props.params;
  const page = source.getPage(params.slug);
  if (!page) notFound();

  const MDX = page.data.body;
  const href = page.url;
  const crumbs = getCrumbs(href);
  const pager = getPagerNeighbors(href);
  const relPath = page.path ?? `${params.slug?.join('/') ?? 'index'}.mdx`;
  const editUrl = `https://github.com/fdarian/agent-dash/edit/main/apps/docs/content/docs/${relPath}`;

  return (
    <div className="ad-shell">
      <DocsSidebar />
      <main className="ad-main">
        <div className="ad-crumbs">
          {crumbs.map((c, i) => (
            <span key={i} style={{ display: 'contents' }}>
              {i > 0 && <span className="sep">/</span>}
              {c.href ? (
                <Link href={c.href}>{c.label}</Link>
              ) : (
                <span className={i === crumbs.length - 1 ? 'cur' : ''}>{c.label}</span>
              )}
            </span>
          ))}
        </div>

        <h1 className="title">{renderTitle(page.data.title)}</h1>
        {page.data.description && <p className="subtitle">{page.data.description}</p>}

        <MDX
          components={getMDXComponents({
            a: createRelativeLink(source, page),
          })}
        />

        {(pager.prev || pager.next) && (
          <div className="ad-pager">
            {pager.prev ? (
              <Link href={pager.prev.href}>
                <div className="hint">← previous</div>
                <div className="ttl">{pager.prev.title}</div>
              </Link>
            ) : (
              <span />
            )}
            {pager.next ? (
              <Link href={pager.next.href} className="next">
                <div className="hint">next →</div>
                <div className="ttl">{pager.next.title}</div>
              </Link>
            ) : (
              <span />
            )}
          </div>
        )}
      </main>
      <DocsToc
        entries={page.data.toc.map((t) => ({
          depth: t.depth,
          title: toText(t.title),
          url: t.url,
        }))}
        meta={{ updated: '2026-04-18', version: 'v0.9.2', readTime: '4 min', editUrl }}
      />
      <footer className="ad-bottom wide">
        <div className="lhs">
          <span>agent-dash</span>
          <span>v0.9.2</span>
          <span>·</span>
          <span>Apache-2.0</span>
        </div>
        <div className="rhs">
          <a href="/">home</a>
          <a href="https://github.com/fdarian/agent-dash/releases">changelog</a>
          <a href="https://github.com/fdarian/agent-dash">github</a>
          <span>MMXXVI</span>
        </div>
      </footer>
    </div>
  );
}

export async function generateStaticParams() {
  return source.generateParams();
}

export async function generateMetadata(props: {
  params: Promise<{ slug?: string[] }>;
}): Promise<Metadata> {
  const params = await props.params;
  const page = source.getPage(params.slug);
  if (!page) notFound();

  return {
    title: page.data.title,
    description: page.data.description,
  };
}
