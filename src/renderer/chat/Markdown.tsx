// Minimal markdown renderer: paragraphs, headings, bullet lists, **bold**,
// `code`, fenced code blocks (with language label + Copy) and placeholder
// cards for structured block languages (see blocks.ts).
import { useState } from 'preact/hooks';
import type { ComponentChildren } from 'preact';
import { parseBlocks, prettyCard, type Segment } from './blocks';
import { t } from '../i18n';

function renderInline(text: string): ComponentChildren {
  // Tokenize on inline code first so bold markers inside code stay literal.
  const parts: ComponentChildren[] = [];
  const re = /(`[^`\n]+`)|(\*\*[^*\n]+\*\*)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    if (m.index > last) parts.push(text.slice(last, m.index));
    if (m[1]) parts.push(<code class="inline">{m[1].slice(1, -1)}</code>);
    else if (m[2]) parts.push(<strong>{m[2].slice(2, -2)}</strong>);
    last = m.index + m[0].length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts;
}

function renderText(text: string): ComponentChildren {
  const blocks = text.split(/\n{2,}/);
  return blocks.map((block, idx) => {
    const lines = block.split('\n').filter((l) => l.trim() !== '');
    if (!lines.length) return null;
    const heading = lines.length === 1 && lines[0].match(/^(#{1,4})\s+(.*)$/);
    if (heading) {
      const level = heading[1].length;
      const Tag = (`h${Math.min(level + 2, 6)}` as unknown) as 'h3';
      return <Tag key={idx}>{renderInline(heading[2])}</Tag>;
    }
    if (lines.every((l) => /^\s*([-*•]|\d+[.)])\s+/.test(l))) {
      const ordered = /^\s*\d+[.)]\s+/.test(lines[0]);
      const items = lines.map((l, i) => <li key={i}>{renderInline(l.replace(/^\s*([-*•]|\d+[.)])\s+/, ''))}</li>);
      return ordered ? <ol key={idx}>{items}</ol> : <ul key={idx}>{items}</ul>;
    }
    return (
      <p key={idx}>
        {lines.map((l, i) => (
          <span key={i}>
            {renderInline(l)}
            {i < lines.length - 1 ? <br /> : null}
          </span>
        ))}
      </p>
    );
  });
}

function CopyButton({ text }: { text: string }) {
  const [done, setDone] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setDone(true);
      setTimeout(() => setDone(false), 1500);
    } catch {
      /* clipboard unavailable */
    }
  };
  return (
    <button class="copy" type="button" onClick={copy}>
      {done ? t('md.copied') : t('md.copy')}
    </button>
  );
}

function CodeBlock({ seg }: { seg: Extract<Segment, { type: 'code' }> }) {
  return (
    <div class={`codeblock${seg.open ? ' open' : ''}`}>
      <div class="codebar">
        <span class="lang">{seg.lang || 'text'}</span>
        <CopyButton text={seg.code} />
      </div>
      <pre>
        <code>{seg.code}</code>
      </pre>
    </div>
  );
}

function CardBlock({ seg }: { seg: Extract<Segment, { type: 'card' }> }) {
  return (
    <div class="card-placeholder">
      <div class="card-title">{t('md.block', { lang: seg.lang })}</div>
      <pre>
        <code>{prettyCard(seg)}</code>
      </pre>
    </div>
  );
}

export function Markdown({ text }: { text: string }) {
  const segments = parseBlocks(text);
  return (
    <div class="md">
      {segments.map((seg, i) => {
        if (seg.type === 'text') return <div key={i}>{renderText(seg.text)}</div>;
        if (seg.type === 'code') return <CodeBlock key={i} seg={seg} />;
        return <CardBlock key={i} seg={seg} />;
      })}
    </div>
  );
}
