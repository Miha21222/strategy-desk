/**
 * Types for data/prop-firms.json (schema strategy-desk/prop-firms/v1) and lookup helpers.
 * Only the fields the simulator reads are typed; everything else is passed through.
 */

export type Family = 'futures' | 'forex';

export interface CostRule {
  structure: 'per30d' | 'onetime' | string;
  windowDays?: number;
  evalFull: number;
  renewal?: number;
  [k: string]: unknown;
}

export interface DrawdownRule {
  amount: number;
  type: 'intraday' | 'eod';
  includesUnrealized?: boolean;
  stopsTrailing?: { lockAtStartPlus: number } | null;
}

export interface FuturesFunded {
  drawdown?: DrawdownRule;
  bufferAmount?: number | null;
  minDaysToFirstPayout?: number;
  payoutFrequencyDays?: number;
  minPayout?: number;
  maxPayoutPct?: number;
  payoutCap?: number;
  maxPayoutsLifetime?: number;
  consistencyMaxDayPct?: number | null;
  activationFee?: number;
  isSim?: boolean;
  note?: string;
  [k: string]: unknown;
}

export interface FuturesAccount {
  id: string;
  label: string;
  accountSize: number;
  profitTarget: number;
  drawdown: DrawdownRule;
  dailyLossLimit: number | null;
  dailySoftLossLimit?: number | null;
  maxContracts: number;
  minTradingDays: number;
  consistencyMaxDayPct: number | null;
  cost: CostRule;
  funded?: FuturesFunded;
  maxFundedAccounts?: number | null;
  [k: string]: unknown;
}

export interface ForexPhase {
  name: string;
  targetPct: number;
  minTradingDays: number;
}

export interface ForexFunded {
  profitSplitByCycle?: Record<string, number>;
  firstPayoutDays?: number;
  payoutCycleDays?: number;
  onDemandConsistencyMaxDayPct?: number | null;
  strikeFloatingLossPct?: number | null;
  activationFee?: number;
  [k: string]: unknown;
}

export interface ForexAccount {
  id: string;
  label: string;
  accountSize: number;
  currency?: string;
  phases: ForexPhase[];
  dailyLoss: { pct: number; basis: string; resetUtcOffset?: number };
  maxLoss: { pct: number; type: 'static' | 'trailing'; includesFloating?: boolean };
  consistencyMaxDayPct: number | null;
  cost: CostRule;
  funded?: ForexFunded;
  [k: string]: unknown;
}

export type Account = FuturesAccount | ForexAccount;

export interface Firm {
  id: string;
  name: string;
  family: Family;
  maxFundedAccounts?: number | null;
  accounts: Account[];
  [k: string]: unknown;
}

export interface PropFirmsData {
  schema: string;
  updated?: string;
  simCaveat?: string;
  firms: Firm[];
  families?: Record<string, string>;
}

export function isForexAccount(a: Account): a is ForexAccount {
  return Array.isArray((a as ForexAccount).phases);
}

export function isFuturesAccount(a: Account): a is FuturesAccount {
  return !isForexAccount(a);
}

export interface AccountRef {
  firm: Firm;
  account: Account;
}

/** Flatten firms → accounts, optionally filtered by firm/account ids and family. */
export function listAccounts(rules: PropFirmsData, filter?: { ids?: string[] | null; family?: Family }): AccountRef[] {
  const out: AccountRef[] = [];
  const ids = filter?.ids && filter.ids.length ? new Set(filter.ids) : null;
  for (const firm of rules.firms) {
    if (filter?.family && firm.family !== filter.family) continue;
    for (const account of firm.accounts) {
      if (ids && !ids.has(firm.id) && !ids.has(account.id)) continue;
      out.push({ firm, account });
    }
  }
  return out;
}

export function findAccount(rules: PropFirmsData, id: string): AccountRef | null {
  for (const firm of rules.firms) for (const account of firm.accounts) if (account.id === id) return { firm, account };
  return null;
}
