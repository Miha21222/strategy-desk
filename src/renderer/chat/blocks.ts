// Fenced-block parser for assistant markdown. Splits text into plain
// segments, code blocks and "card" blocks (structured JSON the stage-2 UI
// will render natively). Tolerates an unterminated fence while streaming.

export const CARD_LANGS = ['nextsteps', 'chartcard', 'stressreport', 'prodata', 'briefreport', 'healthdata', 'workflowstate'] as const;
export type CardLang = (typeof CARD_LANGS)[number];

export type Segment =
  | { type: 'text'; text: string }
  | { type: 'code'; lang: string; code: string; open: boolean }
  | { type: 'card'; lang: CardLang; raw: string; json: unknown | null; open: boolean };

const FENCE_RE = /^(`{3,}|~{3,})\s*([\w+.#-]*)\s*$/;

export function isCardLang(lang: string): lang is CardLang {
  return (CARD_LANGS as readonly string[]).includes(lang.toLowerCase());
}

function tryJson(raw: string): unknown | null {
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

export function parseBlocks(md: string): Segment[] {
  const out: Segment[] = [];
  const lines = md.split(/\r?\n/);
  let text: string[] = [];
  let i = 0;

  const flushText = () => {
    const joined = text.join('\n');
    if (joined.trim()) out.push({ type: 'text', text: joined });
    text = [];
  };

  while (i < lines.length) {
    const m = lines[i].match(FENCE_RE);
    if (!m) {
      text.push(lines[i]);
      i++;
      continue;
    }
    const fence = m[1];
    const lang = (m[2] ?? '').trim();
    const body: string[] = [];
    let closed = false;
    // Closing fence: same character, at least as long as the opener, nothing else.
    const closeRe = new RegExp(`^${fence[0] === '`' ? '`' : '~'}{${fence.length},}\\s*$`);
    i++;
    while (i < lines.length) {
      const line = lines[i];
      if (closeRe.test(line)) {
        closed = true;
        i++;
        break;
      }
      body.push(line);
      i++;
    }
    flushText();
    const code = body.join('\n');
    if (isCardLang(lang)) {
      out.push({ type: 'card', lang: lang.toLowerCase() as CardLang, raw: code, json: closed ? tryJson(code) : null, open: !closed });
    } else {
      out.push({ type: 'code', lang, code, open: !closed });
    }
  }
  flushText();
  return out;
}

/** Pretty-print JSON for the placeholder card; falls back to the raw text. */
export function prettyCard(seg: Extract<Segment, { type: 'card' }>): string {
  return seg.json !== null ? JSON.stringify(seg.json, null, 2) : seg.raw;
}
