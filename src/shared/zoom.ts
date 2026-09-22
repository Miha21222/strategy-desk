// Window zoom: the steps the settings offer, the walk Ctrl +/- makes and the
// clamp the main process applies. Shared so the dropdown, the shortcut and the
// handler can never disagree about what a valid scale is.

export const ZOOM_MIN = 0.8;
export const ZOOM_MAX = 2;

/** Offered scales, in order. 1 = the px sizes the stylesheet was written in. */
export const ZOOM_STEPS = [0.8, 0.9, 1, 1.1, 1.25, 1.4, 1.6, 1.8, 2];

/** Nearest offered step one notch up (`1`) or down (`-1`) from `current`. */
export function nextZoom(current: number, dir: 1 | -1): number {
  const i = ZOOM_STEPS.reduce((best, z, idx) => (Math.abs(z - current) < Math.abs(ZOOM_STEPS[best] - current) ? idx : best), 0);
  return ZOOM_STEPS[Math.min(ZOOM_STEPS.length - 1, Math.max(0, i + dir))];
}

/** Keep the window readable in both directions; a bad number falls back to 1. */
export function clampZoom(factor: unknown): number {
  const n = Number(factor);
  if (!Number.isFinite(n)) return 1;
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, n));
}
