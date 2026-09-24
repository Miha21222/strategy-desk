// Which driver answers a chat. One place, so `agent.ts` never branches on the
// engine id and a new engine is one line here plus its own file.
import type { EngineId } from '@shared/engine';
import { claudeDriver } from './claude';
import { codexDriver } from './codex';
import type { EngineDriver } from './types';

const DRIVERS: Record<EngineId, EngineDriver | null> = {
  claude: claudeDriver,
  codex: codexDriver,
};

export class UnsupportedEngineError extends Error {
  constructor(public readonly engine: string) {
    // The renderer turns this into an i18n string; the message is a key, not prose.
    super(`engine-not-available:${engine}`);
  }
}

export function getDriver(engine: EngineId): EngineDriver {
  const d = DRIVERS[engine];
  if (!d) throw new UnsupportedEngineError(engine);
  return d;
}

/** Every driver that exists, for the app-wide "drop everything" paths. */
export function allDrivers(): EngineDriver[] {
  return Object.values(DRIVERS).filter((d): d is EngineDriver => Boolean(d));
}

export type { EngineDriver, TurnSpec, DriverEvent, SessionSource } from './types';
