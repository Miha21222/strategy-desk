import { describe, expect, it } from 'vitest';
import { parseBlocks, prettyCard, isCardLang } from './blocks';

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

  it('recognises card languages and parses their JSON', () => {
    const segs = parseBlocks('```nextsteps\n{"items":[{"title":"Check IB"}]}\n```');
    expect(segs).toHaveLength(1);
    const card = segs[0];
    expect(card.type).toBe('card');
    if (card.type !== 'card') return;
    expect(card.lang).toBe('nextsteps');
    expect(card.json).toEqual({ items: [{ title: 'Check IB' }] });
    expect(prettyCard(card)).toContain('"title": "Check IB"');
  });

  it('keeps invalid card JSON as raw text', () => {
    const segs = parseBlocks('```chartcard\nnot json\n```');
    const card = segs[0];
    expect(card.type).toBe('card');
    if (card.type !== 'card') return;
    expect(card.json).toBeNull();
    expect(prettyCard(card)).toBe('not json');
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
