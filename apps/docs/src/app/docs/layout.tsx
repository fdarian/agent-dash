import type { ReactNode } from 'react';
import { DocsNav } from '@/components/docs/docs-nav';

const DOCS_DEFAULT_DARK = `(function(){try{if(!localStorage.getItem('theme')){localStorage.setItem('theme','dark');}var t=localStorage.getItem('theme');if(t==='dark'){document.documentElement.classList.add('dark');document.documentElement.classList.remove('light');}else{document.documentElement.classList.add('light');document.documentElement.classList.remove('dark');}}catch(_){}})();`;

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <>
      <script
        // biome-ignore lint/security/noDangerouslySetInnerHtml: inline theme-init runs before paint to avoid flash
        dangerouslySetInnerHTML={{ __html: DOCS_DEFAULT_DARK }}
      />
      <DocsNav />
      {children}
    </>
  );
}
