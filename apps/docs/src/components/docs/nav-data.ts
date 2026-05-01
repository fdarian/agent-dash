export type NavItem = {
  id: string;
  title: string;
  href: string;
  tick: '◆' | '●' | '○';
};

export type NavGroup = {
  n: string;
  label: string;
  items: NavItem[];
};

export const NAV_GROUPS: NavGroup[] = [
  {
    n: '01',
    label: 'start here',
    items: [
      { id: 'overview', title: 'Overview', href: '/docs', tick: '◆' },
      { id: 'installation', title: 'Installation', href: '/docs/installation', tick: '○' },
      {
        id: 'getting-started',
        title: 'Getting started',
        href: '/docs/getting-started',
        tick: '●',
      },
      {
        id: 'agents',
        title: 'Supported agents',
        href: '/docs/agents',
        tick: '○',
      },
    ],
  },
  {
    n: '02',
    label: 'usage',
    items: [
      { id: 'keybinds', title: 'Keybinds', href: '/docs/keybinds', tick: '○' },
      {
        id: 'copy-mode',
        title: 'Copy mode',
        href: '/docs/keybinds#copy-mode',
        tick: '○',
      },
      {
        id: 'groups',
        title: 'Session groups',
        href: '/docs/getting-started#session-groups',
        tick: '○',
      },
      {
        id: 'search',
        title: 'Search',
        href: '/docs/keybinds#preview-pane',
        tick: '○',
      },
    ],
  },
  {
    n: '03',
    label: 'config',
    items: [
      { id: 'configuration', title: 'Configuration', href: '/docs/configuration', tick: '○' },
      {
        id: 'layout',
        title: 'Layout',
        href: '/docs/configuration#layout',
        tick: '○',
      },
      {
        id: 'formatter',
        title: 'Name formatter',
        href: '/docs/configuration#sessionnameformatter',
        tick: '○',
      },
    ],
  },
  {
    n: '04',
    label: 'internals',
    items: [
      {
        id: 'pipeline',
        title: 'Preview pipeline',
        href: '/docs/configuration#previewscrollmode',
        tick: '○',
      },
      {
        id: 'state',
        title: 'State & cache',
        href: '/docs/configuration#data-storage',
        tick: '○',
      },
      { id: 'detect', title: 'Process detect', href: '/docs#how-it-works', tick: '○' },
    ],
  },
];

export type PagerNeighbors = {
  prev?: { title: string; href: string };
  next?: { title: string; href: string };
};

const FLAT = NAV_GROUPS.flatMap((g) => g.items);

export function getPagerNeighbors(href: string): PagerNeighbors {
  const idx = FLAT.findIndex(
    (it) => it.href === href || it.href.split('#')[0] === href,
  );
  if (idx < 0) return {};
  return {
    prev: idx > 0 ? { title: FLAT[idx - 1].title, href: FLAT[idx - 1].href } : undefined,
    next:
      idx < FLAT.length - 1
        ? { title: FLAT[idx + 1].title, href: FLAT[idx + 1].href }
        : undefined,
  };
}

export type Crumb = { label: string; href?: string };

export function getCrumbs(href: string): Crumb[] {
  const base: Crumb[] = [{ label: 'docs', href: '/docs' }];
  for (const g of NAV_GROUPS) {
    const found = g.items.find((it) => it.href.split('#')[0] === href);
    if (found) {
      return [...base, { label: g.label }, { label: found.title.toLowerCase() }];
    }
  }
  return base;
}
