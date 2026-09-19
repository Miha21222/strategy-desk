// Prompt library: English prompts the app sends on the user's behalf, keyed by
// RU labels in i18n (home.*, catalog). Workflow YAMLs add their own prompts;
// this file holds the app-level chips and the fallback report catalog.

export interface PromptItem {
  id: string;
  /** i18n key for the RU label. */
  labelKey: string;
  prompt: string;
}

const LEVELS_SUFFIX = 'Show key levels as a `chartcard` of type levels, never a markdown table.';

/** «Прочитай мой график» opens a chooser with these five variants. */
export const READ_CHART_VARIANTS: PromptItem[] = [
  {
    id: 'worked',
    labelKey: 'home.variantWorked',
    prompt: `Read my chart (tv_desktop_status, one screenshot, Read the image). I have a trade on it that worked: explain what made it work — structure, the level it launched from, the confirmation — and what I should look for to repeat it. ${LEVELS_SUFFIX}`,
  },
  {
    id: 'lost',
    labelKey: 'home.variantLost',
    prompt: `Read my chart (tv_desktop_status, one screenshot, Read the image). I have a trade on it that lost: tell me honestly what the chart was saying against the trade — structure, the level it ran into, timing — and what filter would have kept me out. ${LEVELS_SUFFIX}`,
  },
  {
    id: 'structure',
    labelKey: 'home.variantStructure',
    prompt: `Read my chart (tv_desktop_status, one screenshot, Read the image) and break down the structure: trend on this timeframe, last swing highs and lows, where the range sits, and which side has the liquidity. ${LEVELS_SUFFIX}`,
  },
  {
    id: 'pattern',
    labelKey: 'home.variantPattern',
    prompt: `Read my chart (tv_desktop_status, one screenshot, Read the image), then quantify the repeating pattern I have marked: use the bars from the chart to count how often it appeared and how it resolved, and report the numbers in a \`chartcard\` (stats + hist). ${LEVELS_SUFFIX}`,
  },
  {
    id: 'look',
    labelKey: 'home.variantLook',
    prompt: `Just look at my chart (tv_desktop_status, one screenshot, Read the image) and tell me what stands out: structure, key levels, indicator readings, anything odd. ${LEVELS_SUFFIX}`,
  },
];

export const BUILD_TEST_PROMPT = 'Build me an EMA pullback strategy for PEPPERSTONE:US500 on the 15-minute chart and backtest it.';

/** «Найди стратегию в сети» picks one of these at random. */
export const WEB_STRATEGIES = [
  'an opening range breakout strategy',
  'an EMA pullback strategy',
  'a VWAP mean reversion strategy',
  'an RSI divergence strategy',
  'a moving average crossover strategy',
  'an inside-bar breakout strategy',
  'a London/New York session strategy',
  'a Bollinger Band squeeze strategy',
  'a pivot point reversal strategy',
  'a gap-and-go strategy',
  'the ICT Silver Bullet strategy',
  'an ICT fair value gap (FVG) strategy',
  'an ICT order block + FVG strategy',
  'an ICT liquidity sweep reversal strategy',
  'the ICT Power of Three (AMD) strategy',
  'an ICT breaker block strategy',
];

export function webStrategyPrompt(pick = Math.floor(Math.random() * WEB_STRATEGIES.length)): string {
  const x = WEB_STRATEGIES[Math.max(0, Math.min(WEB_STRATEGIES.length - 1, pick))];
  return `Search the web for ${x}, tell me the source, translate it to Pine Script v6, and backtest it on my current chart.`;
}

export const DEMO_IB_PROMPT = [
  'Build and backtest an Initial Balance breakout strategy for PEPPERSTONE:US500 on the 15-minute chart. Exact spec:',
  '- RTH opens at 16:30 Europe/Kyiv (09:30 ET). Initial Balance = high/low of the first 60 minutes of RTH.',
  '- Entry: a 15m close beyond the IB high (long) or below the IB low (short), only in the direction of a `Daily bias` input (long / short / both).',
  '- Stop loss at the IB midpoint. Target = IB height × 1 (input `Target IB multiple`).',
  '- One trade per day. Flatten everything at 22:55 Kyiv.',
  '- Skip the day when IB height > 0.8 × ADR(5) (input `Max IB / ADR`; ADR = average daily range of the last 5 RTH sessions).',
  '- Expose everything as inputs with titles and groups: session times, IB length, bias, target multiple, max IB/ADR, date window.',
  '- margin_long = margin_short = 0, fixed 1 contract, commission via strategy.commission.cash_per_order.',
  'Show the full Pine v6 script first, then deploy with tv_desktop_pine_build_and_backtest. Sanity check: roughly 0–1 trades per day; anything far off means the session or IB logic is broken — diagnose before reporting.',
].join('\n');

export const MARK_LEVELS_PROMPT = `Mark the key levels on my chart: read it (tv_desktop_status, one screenshot, list my drawings), propose the levels with names and prices in a \`chartcard\` of type levels, and ask me to confirm before drawing anything with tv_desktop_draw.`;

export const PREMARKET_PROMPT =
  'Build me a pre-market indicator in Pine v6 for my current chart: previous day high/low/close, overnight (ETH) high/low, midnight open, London open, and the current Initial Balance once RTH starts. Every level is a toggleable input with its own colour and label. Show the code first, then add it to the chart with tv_desktop_pine_build_and_backtest and read back what landed.';

export const SMART_ALERT_PROMPT =
  'Set up a smart alert for me: ask which level or condition matters right now (or take it from my drawings), then create a TradingView alert with the official server (mcp-tv-create-alert) or a Pine alertcondition if the condition needs indicator logic. Read the alert back and confirm exactly what will fire.';

/** Home chips shown on an empty chat. `chooser` opens the read-chart variants. */
export const HOME_CHIPS: { id: string; labelKey: string; prompt?: string; chooser?: boolean; random?: boolean }[] = [
  { id: 'readChart', labelKey: 'home.readChart', chooser: true },
  { id: 'buildTest', labelKey: 'home.buildTest', prompt: BUILD_TEST_PROMPT },
  { id: 'findWeb', labelKey: 'home.findWeb', random: true },
  { id: 'demoIb', labelKey: 'home.demoIb', prompt: DEMO_IB_PROMPT },
  { id: 'markLevels', labelKey: 'home.markLevels', prompt: MARK_LEVELS_PROMPT },
  { id: 'premarket', labelKey: 'home.premarket', prompt: PREMARKET_PROMPT },
  { id: 'smartAlert', labelKey: 'home.smartAlert', prompt: SMART_ALERT_PROMPT },
];

/** Report catalog (RU label → EN prompt). Typing `/` in the composer lists it. All free. */
export const REPORT_CATALOG: { id: string; label: string; prompt: string }[] = [
  {
    id: 'stress',
    label: 'Стресс-тест',
    prompt:
      'Stress-test the strategy currently on my chart: baseline, higher timeframe, lower timeframe, a correlated symbol and an out-of-sample date split tagged role "oos". Grade it honestly and emit a `stressreport` block.',
  },
  {
    id: 'costs',
    label: 'Проверка издержек',
    prompt:
      'Cost reality check: rerun the current strategy\'s backtest with realistic commission and 1-2 ticks of slippage per side, compare net profit and profit factor with the frictionless run, and emit a `stressreport` block with `costs` filled in.',
  },
  {
    id: 'propTest',
    label: 'Проп-тест',
    prompt:
      'Prop-firm pass test: fetch the closed trades of the current strategy with tv_desktop_read_strategy(orders=2000) and emit a `prodata` block with focus "survival". Do not compute survival odds in prose; the app does the math.',
  },
  {
    id: 'gameplan',
    label: 'Проп-геймплан',
    prompt:
      'Prop-firm gameplan: fetch the closed trades of the current strategy and emit a `prodata` block with focus "gameplan" and the firm from my profile in `gameplan.firm`. The app builds the plan; keep prose to one line.',
  },
  {
    id: 'plateau',
    label: 'Плато параметров',
    prompt:
      'Parameter plateau: take the 2-4 levers of the current strategy, vary each one across a sensible range with tv_desktop_set_study_inputs (not rebuilds), re-read results after each change and show net profit per value as a `chartcard` with bars. Say whether the edge sits on a plateau or a spike.',
  },
  {
    id: 'regime',
    label: 'Режим рынка',
    prompt:
      'Market regime: split the backtest window of the current strategy into trending and ranging periods (ADX or a simple range/expansion filter), report trades, win rate and profit factor per regime as a `chartcard`, and say in one line which regime carries the edge.',
  },
  {
    id: 'healthCheck',
    label: 'Проверка здоровья',
    prompt:
      'Health check of the strategy currently on my chart: fetch its closed trades with tv_desktop_read_strategy and emit a `healthdata` block. The app computes the diagnostics; keep prose to one sentence.',
  },
  {
    id: 'brief',
    label: 'Утренний бриф',
    prompt:
      'Morning brief: work through my morning checklist against the live chart, check each level with tv_desktop_check_levels using the right `since`, pull today\'s calendar, and finish with a `briefreport` block plus a 2-3 line synthesis.',
  },
  {
    id: 'full',
    label: 'Полный отчёт',
    prompt:
      'Full report on the current strategy: fetch its closed trades with tv_desktop_read_strategy(orders=2000) and emit a `prodata` block with focus "full". The app renders the report; keep prose to one line.',
  },
];

/** Health-check prompt for a strategy chat on the shelf (hover action). */
export function healthCheckPrompt(title: string): string {
  return `Health check of the strategy "${title}": fetch its closed trades with tv_desktop_read_strategy(orders=2000) and emit a \`healthdata\` block. The app computes the diagnostics; keep prose to one sentence.`;
}

/** Filter the catalog by the text typed after `/`. */
export function filterCatalog<T extends { label: string; id: string }>(items: T[], query: string): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return items;
  return items.filter((i) => i.label.toLowerCase().includes(q) || i.id.toLowerCase().includes(q));
}
