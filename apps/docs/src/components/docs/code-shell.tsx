'use client';

import { useState } from 'react';

export function CodeShell(props: { lang: string; lines: string[]; raw: string }) {
  const [copied, setCopied] = useState(false);

  return (
    <div className="ad-code">
      <div className="ad-code-head">
        <span className="lang">{props.lang}</span>
        <button
          type="button"
          className="copy"
          onClick={() => {
            navigator.clipboard?.writeText(props.raw);
            setCopied(true);
            setTimeout(() => setCopied(false), 1200);
          }}
        >
          {copied ? '✓ copied' : 'copy'}
        </button>
      </div>
      <div className="ad-code-body">
        {props.lines.map((line, i) => (
          <div className="line" key={i}>
            <span className="gutter">{i + 1}</span>
            <span className="x">{line.length > 0 ? line : ' '}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
