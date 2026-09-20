// The connection lights must not be reassuring when they cannot be sure.
//
// "Something answers on the debug port" was read as "TradingView is there":
// any Chromium app can own that port (on this machine FX Replay and a browser
// panel both do), and the light went green while every chart tool failed
// later with a confusing error. The owner of the port now travels with the
// status so the light can say what is actually true.
import { describe, expect, it, vi } from 'vitest';
import type { CdpProbe } from '@shared/preflight';

vi.mock('electron', () => ({ app: { getPath: () => '/tmp/desk-test' } }));

const { cdpOwnerLabel } = await import('./status');

const target = (over: Partial<CdpProbe['targets'][number]> = {}) => ({ url: '', title: '', ...over });

const probe = (over: Partial<CdpProbe> = {}): CdpProbe => ({
  up: true,
  browser: 'Chrome/150.0.7871.46',
  chartTab: false,
  tradingview: false,
  targets: [],
  ...over,
});

describe('cdpOwnerLabel', () => {
  it('prefers the tab title, which is what a person recognises', () => {
    expect(
      cdpOwnerLabel(probe({ targets: [target({ title: 'FX Replay - Chart', url: 'https://app.fxreplay.com/' })] })),
    ).toBe('FX Replay - Chart');
  });

  it('falls back to the host when the tab has no title', () => {
    expect(cdpOwnerLabel(probe({ targets: [target({ url: 'https://app.fxreplay.com/chart/1' })] }))).toBe(
      'app.fxreplay.com',
    );
  });

  it('falls back to the browser string when there is no target at all', () => {
    expect(cdpOwnerLabel(probe())).toBe('Chrome/150.0.7871.46');
  });

  it('never returns an empty label - an unnamed owner is still worth reporting', () => {
    expect(cdpOwnerLabel(probe({ browser: '', targets: [] }))).toBeTruthy();
  });

  it('keeps a long title readable', () => {
    const long = 'x'.repeat(200);
    expect(cdpOwnerLabel(probe({ targets: [target({ title: long })] })).length).toBeLessThanOrEqual(60);
  });

  it('survives a malformed url instead of throwing', () => {
    expect(cdpOwnerLabel(probe({ targets: [target({ url: 'not a url' })] }))).toBe('not a url');
  });
});
