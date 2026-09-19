import { describe, expect, it } from 'vitest';
import { isPineLang, tokenizePine } from './pine';

describe('tokenizePine', () => {
  it('classifies comments, strings, numbers, keywords and namespaces', () => {
    const toks = tokenizePine('//@version=6\nstrategy("EMA pullback", overlay=true)\nlen = input.int(20, "len") // lever\nif close > ta.ema(close, len)\n    strategy.entry("L", strategy.long)');
    const by = (type: string) => toks.filter((t) => t.type === type).map((t) => t.text);
    expect(by('cmt')).toEqual(['//@version=6', '// lever']);
    expect(by('str')).toEqual(['"EMA pullback"', '"len"', '"L"']);
    expect(by('num')).toEqual(['20']);
    expect(by('kw')).toEqual(['true', 'if']);
    expect(by('ns')).toContain('input');
    expect(by('ns')).toContain('ta');
    expect(by('ns')).toContain('strategy');
    expect(by('fn')).toContain('ema');
  });

  it('round-trips the source text', () => {
    const src = 'x := nz(a[1], 0.5) ? "yes" : \'no\'\n';
    expect(tokenizePine(src).map((t) => t.text).join('')).toBe(src);
  });

  it('marks types only when not used as a namespace', () => {
    const toks = tokenizePine('int a = 1\ncolor.new(color.red, 50)');
    expect(toks.find((t) => t.text === 'int')?.type).toBe('type');
    expect(toks.filter((t) => t.text === 'color').every((t) => t.type === 'ns')).toBe(true);
  });

  it('isPineLang accepts common aliases', () => {
    expect(isPineLang('pine')).toBe(true);
    expect(isPineLang('PineScript')).toBe(true);
    expect(isPineLang('ts')).toBe(false);
  });
});
