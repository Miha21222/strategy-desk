// A canned assistant message containing one valid instance of every block.
// Used by the vitest contract test and by `DESK_SMOKE=blocks`.

export const SAMPLE_BLOCKS: Record<string, unknown> = {
  nextsteps: {
    options: [
      { label: 'Добавить в TradingView', prompt: 'Deploy this strategy to my chart, compile it and run the backtest.' },
      { label: 'Сохранить как стратегию', action: 'save_strategy', name: 'EMA pullback US500 15m' },
      { label: 'Запомнить стиль', action: 'update_profile', text: 'Prefers one trade per day, RTH only.' },
    ],
  },
  chartcard: {
    title: 'US500 15m — ключевые уровни',
    charts: [
      { type: 'stats', items: [{ label: 'Trades', value: 42 }, { label: 'Net', value: '-$320' }, { label: 'PF', value: '1.85' }] },
      { type: 'bars', label: 'PnL по часам', format: 'money', items: [{ label: '09:30', value: -320 }, { label: '10:00', value: 540 }] },
      { type: 'donut', label: 'Win rate', pct: 61, sub: '42 сделки' },
      { type: 'hist', label: 'R распределение', bins: [{ label: '<-1', count: 6 }, { label: '-1..0', count: 10 }, { label: '0..1', count: 14 }, { label: '>1', count: 12 }] },
      { type: 'levels', label: 'Уровни', last: 7544.75, items: [{ label: 'PDH', price: 7555 }, { label: 'IBH', price: 7550.25 }, { label: 'PDL', price: 7520 }] },
    ],
    note: 'Цена под PDH, IB ещё не пробит.',
  },
  stressreport: {
    strategy: 'EMA pullback',
    version: 'v1',
    grade: 'B',
    headline: 'Edge survives timeframes but not the correlated symbol.',
    changes: null,
    unit: 'mini',
    costs: { commissionRT: 4, slippageTicksPerSide: 1, tickValue: 5 },
    runs: [
      { label: 'US500 15m — baseline', trades: 42, winRate: 61, profitFactor: 1.85, netProfit: 4543, maxDrawdownPct: 9.5, role: 'baseline' },
      { label: 'US500 1h', trades: 18, winRate: 55, profitFactor: 1.4, netProfit: 1800, maxDrawdownPct: 12, role: 'variant' },
      { label: 'US500 5m', trades: 120, winRate: 48, profitFactor: 1.05, netProfit: 620, maxDrawdownPct: 18, role: 'variant' },
      { label: 'NAS100 15m', trades: 39, winRate: 44, profitFactor: 0.9, netProfit: -410, maxDrawdownPct: 22, role: 'variant' },
      { label: 'US500 15m — OOS 2025', trades: 20, winRate: 60, profitFactor: 1.3, netProfit: 900, maxDrawdownPct: 8, role: 'oos' },
    ],
    notes: ['NAS100 breaks the edge: session filter likely index-specific.'],
  },
  prodata: {
    strategy: 'EMA pullback',
    version: 'v1',
    market: 'futures',
    contractsInBacktest: 1,
    backtestUnit: 'mini',
    pointValue: 50,
    accountSize: 10000,
    trades: [
      { d: '2025-01-06', ed: '2025-01-06', t: '10:15', pnl: 250, r: 1.0 },
      { d: '2025-01-07', ed: '2025-01-07', t: '09:45', pnl: -120.5, r: -0.5 },
    ],
    firms: null,
    focus: 'survival',
    gameplan: { firm: 'fundingpips-10k-2step-standard' },
    source: 'backtest',
    newsDates: null,
    daily: null,
  },
  briefreport: {
    date: '2026-09-19',
    symbol: 'PEPPERSTONE:US500',
    session: 'London',
    regime: null,
    levels: [
      { name: 'London High', price: 7560, hit: true, note: 'swept 09:40' },
      { name: 'PDL', price: 7520, hit: false },
      { name: 'NY midnight open', price: 7538.5, hit: null },
    ],
    volume: { text: 'выше среднего', vsAvg: 1.1 },
    news: [{ time: '15:30', title: 'CPI', impact: 'high' }, { time: '17:00', title: 'Michigan sentiment', impact: 'low' }],
    states: [{ name: 'Bias', value: 'LONG' }, { name: 'Structure', value: 'HH/HL' }],
    notes: 'CPI в 15:30 — до него без входов.',
  },
  healthdata: {
    strategy: 'EMA pullback',
    contractsInBacktest: 1,
    trades: [{ d: '2025-01-06', pnl: 250, r: 1.0 }, { d: '2025-01-07', pnl: -120.5, r: -0.5 }],
  },
  workflowstate: {
    workflow: 'ib-es',
    date: '2026-09-19',
    phase: 'ib',
    verdict: 'WAIT',
    bias: { direction: 'LONG', confidence: 0.6 },
    levels: [{ name: 'IBH', price: 7550.25 }, { name: 'IBL', price: 7531 }],
    signals: [{ setup: '1', entry: 7551, sl: 7540.5, tp1: 7570, rr: 1.8, validUntil: '17:30' }],
    flags: ['CPI 15:30'],
  },
};

/** One markdown message with a prose line and every block fenced by its name. */
export function sampleAssistantMessage(): string {
  const parts = ['Вот сводка по стратегии.', '', '```pine', '//@version=6', 'strategy("EMA pullback", overlay=true)', 'len = input.int(20, "EMA length")', '```'];
  for (const [kind, data] of Object.entries(SAMPLE_BLOCKS)) {
    parts.push('', `\`\`\`${kind}`, JSON.stringify(data, null, 2), '```');
  }
  return parts.join('\n');
}
