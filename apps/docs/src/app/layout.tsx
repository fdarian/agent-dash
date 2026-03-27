import { Archivo } from 'next/font/google';
import { RootProvider } from 'fumadocs-ui/provider/next';
import type { ReactNode } from 'react';
import './global.css';

const archivo = Archivo({
  subsets: ['latin'],
  variable: '--font-sans',
});

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" className={archivo.variable} suppressHydrationWarning>
      <body className="flex flex-col min-h-screen font-sans">
        <RootProvider>
          <div className="noise-overlay" aria-hidden="true" />
          {children}
        </RootProvider>
      </body>
    </html>
  );
}
