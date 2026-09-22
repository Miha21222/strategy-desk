import { describe, expect, it } from 'vitest';
import { addToast, MAX_TOASTS, TOAST_MS, type Toast } from './toasts';

const toast = (id: number, text: string, level: Toast['level'] = 'info'): Toast => ({ id, text, level });

describe('toast stack', () => {
  it('appends in order', () => {
    const list = addToast(addToast([], toast(1, 'a')), toast(2, 'b'));
    expect(list.map((x) => x.text)).toEqual(['a', 'b']);
  });

  it('refreshes instead of stacking the same text twice', () => {
    const list = addToast(addToast([], toast(1, 'Saved')), toast(2, 'Saved'));
    expect(list).toHaveLength(1);
    expect(list[0].id).toBe(2);
  });

  it('keeps at most MAX_TOASTS, dropping the oldest', () => {
    const list = Array.from({ length: MAX_TOASTS + 2 }, (_, i) => i).reduce((acc, i) => addToast(acc, toast(i, `n${i}`)), [] as Toast[]);
    expect(list).toHaveLength(MAX_TOASTS);
    expect(list[0].text).toBe('n2');
  });

  it('gives a failure more time on screen than a confirmation', () => {
    expect(TOAST_MS.error).toBeGreaterThan(TOAST_MS.warn);
    expect(TOAST_MS.warn).toBeGreaterThan(TOAST_MS.info);
  });
});
