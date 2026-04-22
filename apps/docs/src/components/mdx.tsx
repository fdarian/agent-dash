import type { MDXComponents } from 'mdx/types';
import type { AnchorHTMLAttributes, HTMLAttributes } from 'react';
import { Callout } from './docs/callout';
import { Pre } from './docs/code-block';

function Anchor(props: AnchorHTMLAttributes<HTMLAnchorElement>) {
  const href = props.href ?? '';
  const isExternal = /^https?:\/\//.test(href);
  return (
    <a
      {...props}
      className={`inline ${props.className ?? ''}`}
      target={isExternal ? '_blank' : props.target}
      rel={isExternal ? 'noopener noreferrer' : props.rel}
    />
  );
}

function InlineCode(props: HTMLAttributes<HTMLElement>) {
  return <code {...props} className={`inline ${props.className ?? ''}`} />;
}

export function getMDXComponents(components?: MDXComponents): MDXComponents {
  return {
    pre: Pre,
    a: Anchor,
    code: InlineCode,
    Callout,
    ...components,
  };
}

export const useMDXComponents = getMDXComponents;

declare global {
  type MDXProvidedComponents = ReturnType<typeof getMDXComponents>;
}
