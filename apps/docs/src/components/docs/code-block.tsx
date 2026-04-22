import type { HTMLAttributes, ReactElement, ReactNode } from 'react';
import { CodeShell } from './code-shell';

type CodeProps = {
  className?: string;
  children?: ReactNode;
};

function isCodeElement(child: unknown): child is ReactElement<CodeProps> {
  if (!child || typeof child !== 'object') return false;
  const el = child as { type?: unknown; props?: unknown };
  return el.type === 'code' && el.props !== undefined;
}

function extractText(node: ReactNode): string {
  if (node === null || node === undefined || typeof node === 'boolean') return '';
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(extractText).join('');
  if (typeof node === 'object' && 'props' in node) {
    const el = node as { props: { children?: ReactNode } };
    return extractText(el.props.children);
  }
  return '';
}

export function Pre(props: HTMLAttributes<HTMLPreElement>) {
  const child = Array.isArray(props.children) ? props.children[0] : props.children;
  const codeClass = isCodeElement(child) ? (child.props.className ?? '') : '';
  const langMatch = codeClass.match(/language-(\w+)/);
  const lang = langMatch ? langMatch[1] : 'text';
  const raw = extractText(props.children);
  const lines = raw.replace(/\n$/, '').split('\n');

  return <CodeShell lang={lang} lines={lines} raw={raw} />;
}
