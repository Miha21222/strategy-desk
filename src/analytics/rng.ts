/**
 * Deterministic random numbers and small statistics helpers.
 * Every simulation in the analytics module takes a seed so the same input gives the same output.
 */

export type Rng = () => number;

/** mulberry32 — 32-bit PRNG, uniform in [0, 1). */
export function mulberry32(seed: number): Rng {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Percentile with linear interpolation over an ascending-sorted array. p in [0, 100]. */
export function percentile(sorted: readonly number[], p: number): number {
  const n = sorted.length;
  if (n === 0) return NaN;
  if (n === 1) return sorted[0];
  const pos = (Math.min(100, Math.max(0, p)) / 100) * (n - 1);
  const lo = Math.floor(pos);
  const hi = Math.ceil(pos);
  if (lo === hi) return sorted[lo];
  const w = pos - lo;
  return sorted[lo] * (1 - w) + sorted[hi] * w;
}

export function sortAsc(xs: readonly number[]): number[] {
  return [...xs].sort((a, b) => a - b);
}

export function sum(xs: readonly number[]): number {
  let s = 0;
  for (const x of xs) s += x;
  return s;
}

export function mean(xs: readonly number[]): number {
  return xs.length ? sum(xs) / xs.length : 0;
}

/** Sample standard deviation (n − 1). Returns 0 for n < 2. */
export function sd(xs: readonly number[]): number {
  const n = xs.length;
  if (n < 2) return 0;
  const m = mean(xs);
  let s = 0;
  for (const x of xs) s += (x - m) * (x - m);
  return Math.sqrt(s / (n - 1));
}

export function median(xs: readonly number[]): number {
  return percentile(sortAsc(xs), 50);
}

export function clamp(x: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, x));
}

/** Round to `digits` decimals (default 1). */
export function round(x: number, digits = 1): number {
  const k = 10 ** digits;
  return Math.round(x * k) / k;
}

/** Pearson correlation of two equal-length series; 0 when either has zero variance. */
export function pearson(a: readonly number[], b: readonly number[]): number {
  const n = Math.min(a.length, b.length);
  if (n < 2) return 0;
  const ma = mean(a.slice(0, n));
  const mb = mean(b.slice(0, n));
  let sab = 0;
  let saa = 0;
  let sbb = 0;
  for (let i = 0; i < n; i++) {
    const da = a[i] - ma;
    const db = b[i] - mb;
    sab += da * db;
    saa += da * da;
    sbb += db * db;
  }
  if (saa === 0 || sbb === 0) return 0;
  return sab / Math.sqrt(saa * sbb);
}

/** R² of a least-squares line through (index, value). Returns { r2, slope }. */
export function linearFit(ys: readonly number[]): { r2: number; slope: number } {
  const n = ys.length;
  if (n < 2) return { r2: 0, slope: 0 };
  const mx = (n - 1) / 2;
  const my = mean(ys);
  let sxy = 0;
  let sxx = 0;
  let syy = 0;
  for (let i = 0; i < n; i++) {
    const dx = i - mx;
    const dy = ys[i] - my;
    sxy += dx * dy;
    sxx += dx * dx;
    syy += dy * dy;
  }
  if (sxx === 0 || syy === 0) return { r2: 0, slope: 0 };
  const slope = sxy / sxx;
  return { r2: (sxy * sxy) / (sxx * syy), slope };
}

/** Piecewise-linear band: anchors [[x, score], …] sorted by x; clamps outside the range. */
export function band(x: number, anchors: readonly (readonly [number, number])[]): number {
  if (!Number.isFinite(x)) return 0;
  if (anchors.length === 0) return 0;
  if (x <= anchors[0][0]) return anchors[0][1];
  const last = anchors[anchors.length - 1];
  if (x >= last[0]) return last[1];
  for (let i = 1; i < anchors.length; i++) {
    const [x0, y0] = anchors[i - 1];
    const [x1, y1] = anchors[i];
    if (x <= x1) {
      if (x1 === x0) return y1;
      return y0 + ((x - x0) / (x1 - x0)) * (y1 - y0);
    }
  }
  return last[1];
}

/** Quantiles p5/p25/p50/p75/p95 of an unsorted array. */
export function q5(xs: readonly number[]) {
  const s = sortAsc(xs);
  return {
    p5: percentile(s, 5),
    p25: percentile(s, 25),
    p50: percentile(s, 50),
    p75: percentile(s, 75),
    p95: percentile(s, 95),
  };
}

/** Quantiles p25/p50/p75 of an unsorted array. */
export function q3(xs: readonly number[]) {
  const s = sortAsc(xs);
  return { p25: percentile(s, 25), p50: percentile(s, 50), p75: percentile(s, 75) };
}

/** Max drawdown (peak-to-trough, ≥ 0) of a P&L sequence starting from equity 0. */
export function maxDrawdown(pnls: readonly number[]): number {
  let eq = 0;
  let peak = 0;
  let dd = 0;
  for (const p of pnls) {
    eq += p;
    if (eq > peak) peak = eq;
    const d = peak - eq;
    if (d > dd) dd = d;
  }
  return dd;
}

/** Deterministic 32-bit hash of a string (FNV-1a) for deriving seeds from ids. */
export function hashString(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}
