import { describe, expect, it } from 'vitest';
import { clampZoom, nextZoom, ZOOM_MAX, ZOOM_MIN, ZOOM_STEPS } from './zoom';

describe('window zoom', () => {
  it('clamps to the readable range and falls back to 1 on junk', () => {
    expect(clampZoom(0.1)).toBe(ZOOM_MIN);
    expect(clampZoom(9)).toBe(ZOOM_MAX);
    expect(clampZoom(1.25)).toBe(1.25);
    expect(clampZoom('nope')).toBe(1);
    expect(clampZoom(undefined)).toBe(1);
  });

  it('walks the offered steps and stops at both ends', () => {
    expect(nextZoom(1, 1)).toBe(1.1);
    expect(nextZoom(1, -1)).toBe(0.9);
    expect(nextZoom(ZOOM_STEPS[ZOOM_STEPS.length - 1], 1)).toBe(ZOOM_MAX);
    expect(nextZoom(ZOOM_STEPS[0], -1)).toBe(ZOOM_MIN);
  });

  it('snaps a stored value that is not a step onto the nearest one', () => {
    expect(nextZoom(1.03, 1)).toBe(1.1);
  });
});
