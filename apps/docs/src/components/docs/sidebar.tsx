'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { NAV_GROUPS } from './nav-data';

export function DocsSidebar() {
  const pathname = usePathname();

  return (
    <aside className="ad-side-wrap">
      <div className="ad-side">
        {NAV_GROUPS.map((g) => (
          <div className="group" key={g.n}>
            <div className="group-label">
              <span className="n">{g.n}</span>
              <span>{g.label}</span>
              <span className="line" />
            </div>
            {g.items.map((it) => {
              const on = pathname === it.href;
              return (
                <Link key={it.id} href={it.href} className={`item ${on ? 'on' : ''}`}>
                  <span className="tick">{on ? '●' : it.tick}</span>
                  <span>{it.title}</span>
                </Link>
              );
            })}
          </div>
        ))}
      </div>
    </aside>
  );
}
