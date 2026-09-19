import { describe, expect, it } from 'vitest';
import { blocksOfKind, parseBlocks, prettyCard, isCardLang } from './blocks';
import { BLOCK_KINDS } from '@shared/blocks';
import { sampleAssistantMessage } from '@shared/blocks-sample';

describe('parseBlocks', () => {
  it('returns a single text segment for plain markdown', () => {
    const segs = parseBlocks('Hello **world**\n\nSecond paragraph');
    expect(segs).toEqual([{ type: 'text', text: 'Hello **world**\n\nSecond paragraph' }]);
  });

  it('splits fenced code with a language label', () => {
    const segs = parseBlocks('before\n```pine\n//@version=5\nindicator("x")\n```\nafter');
    expect(segs).toHaveLength(3);
    expect(segs[0]).toEqual({ type: 'text', text: 'before' });
    expect(segs[1]).toEqual({ type: 'code', lang: 'pine', code: '//@version=5\nindicator("x")', open: false });
    expect(segs[2]).toEqual({ type: 'text', text: 'after' });
  });

  it('recognises card languages and validates their JSON', () => {
    const segs = parseBlocks('```nextsteps\n{"options":[{"label":"Check IB","prompt":"Check the IB"}]}\n```');
    expect(segs).toHaveLength(1);
    const card = segs[0];
    expect(card.type).toBe('card');
    if (card.type !== 'card') return;
    expect(card.lang).toBe('nextsteps');
    expect(card.parsed?.ok).toBe(true);
    expect(prettyCard(card)).toContain('"label": "Check IB"');
  });

  it('reports invalid card JSON as an error and keeps the raw text', () => {
    const segs = parseBlocks('```chartcard\nnot json\n```');
    const card = segs[0];
    expect(card.type).toBe('card');
    if (card.type !== 'card') return;
    expect(card.parsed?.ok).toBe(false);
    if (card.parsed && !card.parsed.ok) expect(card.parsed.error).toContain('invalid JSON');
    expect(prettyCard(card)).toBe('not json');
  });

  it('reports a schema failure without throwing', () => {
    const segs = parseBlocks('```stressreport\n{"strategy":"x","grade":"Z","headline":"h","runs":[]}\n```');
    const card = segs[0];
    if (card.type !== 'card') throw new Error('expected card');
    expect(card.parsed?.ok).toBe(false);
    if (card.parsed && !card.parsed.ok) expect(card.parsed.error).toMatch(/grade|runs/);
  });

  it('does not parse an open (streaming) card', () => {
    const segs = parseBlocks('```chartcard\n{"title":"t",');
    const card = segs[0];
    if (card.type !== 'card') throw new Error('expected card');
    expect(card.open).toBe(true);
    expect(card.parsed).toBeNull();
  });

  it('parses the canned sample message with zero schema errors', () => {
    const segs = parseBlocks(sampleAssistantMessage());
    const cards = segs.filter((s) => s.type === 'card');
    expect(cards).toHaveLength(BLOCK_KINDS.length);
    for (const c of cards) if (c.type === 'card') expect(c.parsed?.ok, c.lang).toBe(true);
    expect(segs.some((s) => s.type === 'code' && s.lang === 'pine')).toBe(true);
    expect(blocksOfKind(sampleAssistantMessage(), 'stressreport')).toHaveLength(1);
  });

  it('treats an unterminated fence as an open code block (streaming)', () => {
    const segs = parseBlocks('text\n```ts\nconst a = 1;');
    expect(segs).toEqual([
      { type: 'text', text: 'text' },
      { type: 'code', lang: 'ts', code: 'const a = 1;', open: true },
    ]);
  });

  it('does not treat inline backticks as fences', () => {
    const segs = parseBlocks('call `tv_desktop_status` first');
    expect(segs).toEqual([{ type: 'text', text: 'call `tv_desktop_status` first' }]);
  });

  it('supports tilde fences and empty language', () => {
    const segs = parseBlocks('~~~\nplain\n~~~');
    expect(segs).toEqual([{ type: 'code', lang: '', code: 'plain', open: false }]);
  });

  it('isCardLang is case-insensitive and rejects unknown', () => {
    expect(isCardLang('BriefReport')).toBe(true);
    expect(isCardLang('python')).toBe(false);
  });
});
