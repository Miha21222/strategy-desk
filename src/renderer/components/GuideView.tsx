// The user guide inside the Help tab: a purpose-built renderer for exactly the
// markdown README.md (the user guide) uses — headings, paragraphs, **bold**, `code`, flat
// lists, pipe tables, standalone images, links and `---` rules. Nothing else,
// on purpose: chat markdown lives in ../chat/Markdown.tsx and has chat rules.
import type { ComponentChildren } from 'preact';
import { t } from '../i18n';

export interface GuideViewProps {
  md: string;
  /** Image path as written in the markdown ('docs/guide/img/01-x.png') → bundled URL. */
  images: Record<string, string>;
}

type Block =
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'para'; text: string }
  | { kind: 'list'; ordered: boolean; items: string[] }
  | { kind: 'table'; head: string[]; rows: string[][] }
  | { kind: 'image'; alt: string; src: string }
  | { kind: 'rule' };

const LIST_RE = /^\s*(?:[-*]|\d+[.)])\s+/;
const IMAGE_RE = /^!\[([^\]]*)\]\(([^)\s]+)\)\s*$/;
const TABLE_SEP_RE = /^\|?\s*:?-{3,}/;

function cells(line: string): string[] {
  const s = line.trim().replace(/^\|/, '').replace(/\|$/, '');
  return s.split('|').map((c) => c.trim());
}

/** Line-grouped parser: a blank line or a change of block kind ends a block. */
export function parseGuide(md: string): Block[] {
  const lines = md.split('\n');
  const out: Block[] = [];
  let para: string[] = [];
  const flush = () => {
    if (para.length) out.push({ kind: 'para', text: para.join(' ') });
    para = [];
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();
    if (!trimmed) {
      flush();
      continue;
    }
    const heading = trimmed.match(/^(#{1,6})\s+(.*?)\s*#*$/);
    if (heading) {
      flush();
      out.push({ kind: 'heading', level: heading[1].length, text: heading[2] });
      continue;
    }
    if (/^-{3,}$/.test(trimmed)) {
      flush();
      out.push({ kind: 'rule' });
      continue;
    }
    const image = trimmed.match(IMAGE_RE);
    if (image) {
      flush();
      out.push({ kind: 'image', alt: image[1], src: image[2] });
      continue;
    }
    if (trimmed.startsWith('|') && i + 1 < lines.length && TABLE_SEP_RE.test(lines[i + 1].trim())) {
      flush();
      const head = cells(trimmed);
      const rows: string[][] = [];
      i += 2;
      while (i < lines.length && lines[i].trim().startsWith('|')) rows.push(cells(lines[i++]));
      i--;
      out.push({ kind: 'table', head, rows });
      continue;
    }
    if (LIST_RE.test(line)) {
      flush();
      const ordered = /^\s*\d+[.)]\s+/.test(line);
      const items: string[] = [];
      while (i < lines.length && lines[i].trim()) {
        if (LIST_RE.test(lines[i])) items.push(lines[i].replace(LIST_RE, '').trim());
        else if (/^\s+/.test(lines[i]) && items.length) items[items.length - 1] += ' ' + lines[i].trim();
        else break;
        i++;
      }
      i--;
      out.push({ kind: 'list', ordered, items });
      continue;
    }
    para.push(trimmed);
  }
  flush();
  return out;
}

function openExternal(e: Event, url: string): void {
  e.preventDefault();
  void window.desk.open.external(url);
}

/** Inline code first so markers inside code stay literal; then bold, links and autolinks. */
function inline(text: string): ComponentChildren {
  const parts: ComponentChildren[] = [];
  const re = /(`[^`\n]+`)|(\*\*[^*\n]+\*\*)|\[([^\]\n]+)\]\(([^)\s]+)\)|<(https?:\/\/[^>\s]+)>/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    if (m.index > last) parts.push(text.slice(last, m.index));
    if (m[1]) parts.push(<code class="inline">{m[1].slice(1, -1)}</code>);
    else if (m[2]) parts.push(<strong>{inline(m[2].slice(2, -2))}</strong>);
    else if (m[3] !== undefined) parts.push(link(m[3], m[4]));
    else if (m[5]) parts.push(link(m[5], m[5]));
    last = m.index + m[0].length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts;
}

function link(text: string, url: string): ComponentChildren {
  if (/^https?:\/\//.test(url)) {
    return (
      <a href={url} onClick={(e) => openExternal(e, url)}>
        {text}
      </a>
    );
  }
  // In-document anchors (and anything else): the text alone — the sections are a scroll away.
  return text;
}

export function GuideView(p: GuideViewProps) {
  const blocks = parseGuide(p.md);
  return (
    <div class="guide">
      {blocks.map((b, i) => {
        switch (b.kind) {
          case 'heading': {
            // The guide's sections start at `##`; the tab already has an h2 title.
            const Tag = `h${Math.min(b.level + 1, 6)}` as 'h3';
            return <Tag key={i}>{inline(b.text)}</Tag>;
          }
          case 'para':
            return <p key={i}>{inline(b.text)}</p>;
          case 'list': {
            const items = b.items.map((it, j) => <li key={j}>{inline(it)}</li>);
            return b.ordered ? <ol key={i}>{items}</ol> : <ul key={i}>{items}</ul>;
          }
          case 'table':
            return (
              <table key={i}>
                <thead>
                  <tr>
                    {b.head.map((c, j) => (
                      <th key={j}>{inline(c)}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {b.rows.map((r, j) => (
                    <tr key={j}>
                      {r.map((c, k) => (
                        <td key={k}>{inline(c)}</td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            );
          case 'image': {
            if (/\.gif$/i.test(b.src)) return <p key={i} class="muted">{t('helpSimple.gifNote')}</p>;
            const url = p.images[b.src];
            return url ? <img key={i} src={url} alt={b.alt} title={b.alt} /> : null;
          }
          case 'rule':
            return <hr key={i} />;
        }
      })}
    </div>
  );
}
