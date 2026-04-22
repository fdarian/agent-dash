import type { ReactNode } from 'react';

type CalloutKind = 'tip' | 'note' | 'warn';

export function Callout(props: { kind?: CalloutKind; title: string; children: ReactNode }) {
  const kind = props.kind ?? 'tip';
  return (
    <div className={`ad-doc-callout ${kind}`}>
      <div className="ad-doc-callout-head">
        <span className="dot" />
        <span>{props.title}</span>
      </div>
      <div className="ad-doc-callout-body">{props.children}</div>
    </div>
  );
}
