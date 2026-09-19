// Fenced-block parser for assistant markdown. Splits text into plain
// segments, code blocks and "card" blocks (structured JSON validated against
// the shared zod schemas in src/shared/blocks.ts). Tolerates an unterminated
// fence while streaming: an open card is not parsed until its fence closes.
import { BLOCK_KINDS, isBlockKind, parseBlock, type Block, type BlockKind, type ParsedBlock } from '@shared/blocks';

export const CARD_LANGS = BLOCK_KINDS;
export type CardLang = BlockKind;

export type Segment =
  | { type: 'text'; text: string }
  | { type: 'code'; lang: string; code: string; open: boolean }
  | { type: 'card'; lang: CardLang; raw: string; open: boolean; parsed: ParsedBlock | null };

const FENCE_RE = /^(`{3,}|~{3,})\s*([\w+.#-]*)\s*$/;

export const isCardLang = isBlockKind;

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
      const kind = lang.toLowerCase() as CardLang;
      out.push({ type: 'card', lang: kind, raw: code, open: !closed, parsed: closed ? parseBlock(kind, code) : null });
    } else {
      out.push({ type: 'code', lang, code, open: !closed });
    }
  }
  flushText();
  return out;
}

/** Pretty-print the card JSON (for placeholders / error cards); falls back to the raw text. */
export function prettyCard(seg: Extract<Segment, { type: 'card' }>): string {
  if (seg.parsed?.ok) return JSON.stringify(seg.parsed.block.data, null, 2);
  try {
    return JSON.stringify(JSON.parse(seg.raw), null, 2);
  } catch {
    return seg.raw;
  }
}

/** All successfully parsed blocks of one kind in a message (used for report capture). */
export function blocksOfKind<K extends BlockKind>(md: string, kind: K): Extract<Block, { kind: K }>[] {
  const found: Extract<Block, { kind: K }>[] = [];
  for (const seg of parseBlocks(md)) {
    if (seg.type === 'card' && seg.lang === kind && seg.parsed?.ok) found.push(seg.parsed.block as Extract<Block, { kind: K }>);
  }
  return found;
}
