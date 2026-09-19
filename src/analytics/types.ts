/**
 * Shared input types for the analytics module.
 *
 * `Prodata` mirrors the ```prodata block emitted by the model (see prompt/system-prompt.draft.md,
 * "Blocks"). The model only emits raw trades; every number shown to the user is computed here.
 */

export type Market = 'futures' | 'forex' | 'cfd';
export type BacktestUnit = 'mini' | 'micro' | 'lot';
export type Focus = 'survival' | 'full' | 'gameplan';

/** One closed trade as emitted by the model. */
export interface RawTrade {
  /** Exit date YYYY-MM-DD (required; defines the day block). */
  d: string;
  /** Entry date YYYY-MM-DD (optional; `ed < d` marks an overnight trade). */
  ed?: string | null;
  /** Entry time HH:MM (optional; enables hour / killzone / revenge analysis). */
  t?: string | null;
  /** P&L in account currency for the whole backtest size (`contractsInBacktest`). */
  pnl: number;
  /** R-multiple (optional; preferred for forex/cfd). */
  r?: number | null;
}

/** Daily bar for regime / overnight-gap analysis. */
export interface DailyBar {
  d: string;
  o: number;
  c: number;
  h?: number;
  l?: number;
}

export interface Prodata {
  strategy: string;
  version?: string;
  market: Market;
  contractsInBacktest: number;
  backtestUnit: BacktestUnit;
  pointValue?: number | null;
  accountSize?: number | null;
  trades: RawTrade[];
  /** Firm ids or account ids to evaluate; null = every account of the matching family. */
  firms?: string[] | null;
  focus: Focus;
  gameplan?: { firm: string } | null;
  source?: 'backtest' | 'upload' | 'journal' | string;
  newsDates?: string[] | null;
  daily?: DailyBar[] | null;
  /** Number of parameter variants tried before picking this one (selection-luck cap). */
  variants?: number | null;
  /** First out-of-sample exit date (inclusive); trades from here on are OOS. */
  oosStart?: string | null;
}

/** Quantile summary used everywhere. */
export interface Q5 {
  p5: number;
  p25: number;
  p50: number;
  p75: number;
  p95: number;
}
export interface Q3 {
  p25: number;
  p50: number;
  p75: number;
}
