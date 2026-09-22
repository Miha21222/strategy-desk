// Model <-> UI block contract. Every fenced block the copilot emits with one of
// the BLOCK_KINDS languages is parsed against these zod schemas before it is
// rendered. Both the renderer (cards) and the analytics module (prodata /
// healthdata math) import the types from here, so this file is the single
// source of truth for the shape of the JSON. Keep it free of DOM / Electron.
import { z } from 'zod';

export const BLOCK_KINDS = ['nextsteps', 'chartcard', 'stressreport', 'prodata', 'briefreport', 'healthdata', 'workflowstate'] as const;
export type BlockKind = (typeof BLOCK_KINDS)[number];

export const LIMITS = {
  options: 4,
  charts: 6,
  stats: 6,
  bars: 30,
  histBins: 14,
  levels: 14,
  trades: 2000,
  runs: 12,
  notes: 10,
  news: 12,
  states: 8,
  signals: 6,
  flags: 10,
} as const;

/** Characters kept from a `note` before it is trimmed (see `note` below). */
export const NOTE_MAX = 400;

const short = z.string().trim().max(120);
const text = z.string().max(2000);
/**
 * A free-form one-liner under a card (`note`). The model writes prose here and
 * regularly overshoots any hard limit, so an over-long note is trimmed instead
 * of failing the whole block: a card with a shortened takeaway beats an error
 * card with none.
 */
const note = z.preprocess((v) => (typeof v === 'string' ? v.trim().slice(0, NOTE_MAX) : v), z.string().max(NOTE_MAX));
const num = z.number().finite();
const numOrNull = num.nullable();

// ---------- nextsteps ----------

export const NextStepsOptionSchema = z.union([
  z.object({ label: short, prompt: text.min(1) }),
  z.object({ label: short, action: z.enum(['save_strategy', 'save_indicator', 'save_performance']), name: short.optional() }),
  z.object({ label: short, action: z.literal('update_profile'), text: text.min(1) }),
]);

export const NextStepsSchema = z.object({
  options: z.array(NextStepsOptionSchema).min(1).max(LIMITS.options),
});

// ---------- chartcard ----------

export const ChartFormatSchema = z.enum(['money', 'points', 'percent', 'number']);

export const StatsChartSchema = z.object({
  type: z.literal('stats'),
  label: short.optional(),
  items: z.array(z.object({ label: short, value: z.union([z.string().max(60), num]) })).min(1).max(LIMITS.stats),
});
export const BarsChartSchema = z.object({
  type: z.literal('bars'),
  label: short.optional(),
  format: ChartFormatSchema.default('number'),
  items: z.array(z.object({ label: short, value: num })).min(1).max(LIMITS.bars),
});
export const DonutChartSchema = z.object({
  type: z.literal('donut'),
  label: short.optional(),
  pct: num.min(0).max(100),
  sub: short.optional(),
});
export const HistChartSchema = z.object({
  type: z.literal('hist'),
  label: short.optional(),
  bins: z.array(z.object({ label: short, count: num.min(0) })).min(1).max(LIMITS.histBins),
});
export const LevelsChartSchema = z.object({
  type: z.literal('levels'),
  label: short.optional(),
  last: num.optional(),
  items: z.array(z.object({ label: short, price: num, note: note.optional() })).min(1).max(LIMITS.levels),
});

export const ChartSchema = z.discriminatedUnion('type', [StatsChartSchema, BarsChartSchema, DonutChartSchema, HistChartSchema, LevelsChartSchema]);

export const ChartCardSchema = z.object({
  title: short,
  charts: z.array(ChartSchema).min(1).max(LIMITS.charts),
  note: note.nullable().optional(),
});

// ---------- stressreport ----------

export const GradeSchema = z.enum(['A', 'B', 'C', 'D', 'F']);
export const RunRoleSchema = z.enum(['baseline', 'variant', 'oos']);

export const StressRunSchema = z.object({
  label: short,
  trades: num.min(0),
  winRate: num.min(0).max(100),
  profitFactor: num.min(0),
  netProfit: num,
  maxDrawdownPct: num.min(0),
  role: RunRoleSchema.default('variant'),
});

export const CostsSchema = z.object({
  commissionRT: num.min(0),
  slippageTicksPerSide: num.min(0),
  tickValue: num.min(0),
});

export const StressReportSchema = z.object({
  strategy: short,
  version: short.default('v1'),
  grade: GradeSchema,
  headline: text,
  changes: text.nullable().optional(),
  unit: z.enum(['contract', 'lot', 'micro', 'mini']).optional(),
  costs: CostsSchema.nullable().optional(),
  runs: z.array(StressRunSchema).min(1).max(LIMITS.runs),
  notes: z.array(text).max(LIMITS.notes).default([]),
});

// ---------- prodata / healthdata ----------

export const TradeSchema = z.object({
  d: z.string().max(20),
  ed: z.string().max(20).optional(),
  t: z.string().max(8).optional(),
  pnl: num,
  r: numOrNull.optional(),
});

export const ProdataSchema = z.object({
  strategy: short,
  version: short.default('v1'),
  market: z.enum(['futures', 'forex', 'cfd']),
  contractsInBacktest: num.positive().default(1),
  backtestUnit: z.enum(['mini', 'micro', 'lot']).optional(),
  pointValue: num.positive().optional(),
  accountSize: num.positive().optional(),
  trades: z.array(TradeSchema).max(LIMITS.trades),
  firms: z.array(short).nullable().optional(),
  focus: z.enum(['survival', 'full', 'gameplan']).default('full'),
  gameplan: z.object({ firm: short }).nullable().optional(),
  source: z.enum(['backtest', 'upload', 'journal']).default('backtest'),
  newsDates: z.array(z.string().max(20)).nullable().optional(),
  daily: z.array(z.object({ d: z.string().max(20), pnl: num })).nullable().optional(),
});

export const HealthDataSchema = z.object({
  strategy: short,
  contractsInBacktest: num.positive().default(1),
  trades: z.array(TradeSchema).max(LIMITS.trades),
});

// ---------- briefreport ----------

export const BriefReportSchema = z.object({
  date: z.string().max(20),
  symbol: short,
  session: short.optional(),
  regime: short.nullable().optional(),
  levels: z
    .array(z.object({ name: short, price: num, hit: z.boolean().nullable().default(null), note: note.optional() }))
    .max(LIMITS.levels)
    .default([]),
  volume: z.object({ text: short, vsAvg: numOrNull.optional() }).nullable().optional(),
  news: z.array(z.object({ time: z.string().max(12), title: short, impact: z.enum(['high', 'medium', 'low']).default('low') })).max(LIMITS.news).default([]),
  states: z.array(z.object({ name: short, value: short })).max(LIMITS.states).default([]),
  notes: text.nullable().optional(),
});

// ---------- workflowstate ----------

export const WorkflowStateSchema = z.object({
  workflow: short,
  date: z.string().max(20),
  phase: z.enum(['verdict', 'eth-scan', 'or', 'ib', 'breakout']),
  verdict: z.enum(['GO', 'STOP', 'WAIT']),
  bias: z.object({ direction: z.enum(['LONG', 'SHORT', 'NONE']), confidence: num.min(0).max(1) }),
  levels: z.array(z.object({ name: short, price: num, note: note.optional() })).max(LIMITS.levels).default([]),
  signals: z
    .array(
      z.object({
        setup: z.union([z.string().max(12), num]),
        entry: num,
        sl: num,
        tp1: num,
        rr: numOrNull.optional(),
        validUntil: z.string().max(12).optional(),
      }),
    )
    .max(LIMITS.signals)
    .default([]),
  flags: z.array(short).max(LIMITS.flags).default([]),
});

// ---------- registry ----------

export const BLOCK_SCHEMAS = {
  nextsteps: NextStepsSchema,
  chartcard: ChartCardSchema,
  stressreport: StressReportSchema,
  prodata: ProdataSchema,
  briefreport: BriefReportSchema,
  healthdata: HealthDataSchema,
  workflowstate: WorkflowStateSchema,
} as const;

export type NextSteps = z.infer<typeof NextStepsSchema>;
export type NextStepsOption = z.infer<typeof NextStepsOptionSchema>;
export type ChartCard = z.infer<typeof ChartCardSchema>;
export type Chart = z.infer<typeof ChartSchema>;
export type ChartFormat = z.infer<typeof ChartFormatSchema>;
export type StressReport = z.infer<typeof StressReportSchema>;
export type StressRun = z.infer<typeof StressRunSchema>;
export type Grade = z.infer<typeof GradeSchema>;
export type Prodata = z.infer<typeof ProdataSchema>;
export type Trade = z.infer<typeof TradeSchema>;
export type HealthData = z.infer<typeof HealthDataSchema>;
export type BriefReport = z.infer<typeof BriefReportSchema>;
export type WorkflowState = z.infer<typeof WorkflowStateSchema>;

export type Block =
  | { kind: 'nextsteps'; data: NextSteps }
  | { kind: 'chartcard'; data: ChartCard }
  | { kind: 'stressreport'; data: StressReport }
  | { kind: 'prodata'; data: Prodata }
  | { kind: 'briefreport'; data: BriefReport }
  | { kind: 'healthdata'; data: HealthData }
  | { kind: 'workflowstate'; data: WorkflowState };

export type ParsedBlock = { ok: true; block: Block } | { ok: false; kind: BlockKind; error: string };

export function isBlockKind(lang: string): lang is BlockKind {
  return (BLOCK_KINDS as readonly string[]).includes(lang.toLowerCase());
}

const MAX_ISSUES = 4;

function describeIssues(err: z.ZodError): string {
  const lines = err.issues.slice(0, MAX_ISSUES).map((i) => {
    const path = i.path.length ? i.path.map(String).join('.') : '(root)';
    return `${path}: ${i.message}`;
  });
  const more = err.issues.length - lines.length;
  return more > 0 ? `${lines.join('; ')}; +${more} more` : lines.join('; ');
}

/** Parse the raw text of a fenced block. Never throws. */
export function parseBlock(lang: string, raw: string): ParsedBlock {
  const kind = lang.toLowerCase() as BlockKind;
  if (!isBlockKind(kind)) return { ok: false, kind: 'chartcard', error: `unknown block "${lang}"` };
  let json: unknown;
  try {
    json = JSON.parse(raw);
  } catch (e) {
    return { ok: false, kind, error: `invalid JSON: ${(e as Error).message}` };
  }
  const res = BLOCK_SCHEMAS[kind].safeParse(json);
  if (!res.success) return { ok: false, kind, error: describeIssues(res.error) };
  return { ok: true, block: { kind, data: res.data } as Block };
}

// ---------- derived numbers the UI and the analytics side must agree on ----------

/** Round-trip cost of one trade: commission + slippage on both sides. */
export function costPerTrade(costs: NonNullable<StressReport['costs']>): number {
  return costs.commissionRT + 2 * costs.slippageTicksPerSide * costs.tickValue;
}

/** Net profit of a run after the modelled costs (null when no costs given). */
export function netAfterCosts(run: StressRun, costs: StressReport['costs'] | undefined): number | null {
  if (!costs) return null;
  return run.netProfit - run.trades * costPerTrade(costs);
}

/** Trades-weighted win rate across runs (0-100). */
export function weightedWinRate(runs: StressRun[]): number {
  const trades = runs.reduce((n, r) => n + r.trades, 0);
  if (trades <= 0) return 0;
  return runs.reduce((n, r) => n + r.winRate * r.trades, 0) / trades;
}
