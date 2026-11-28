// src/main/drivers/index.ts with one engine: the capture's replay driver.
import type { EngineDriver } from '../../../../../src/main/drivers/types';

export let driver: EngineDriver | null = null;
export function setDriver(d: EngineDriver): void {
  driver = d;
}
export function getDriver(): EngineDriver {
  if (!driver) throw new Error('no driver');
  return driver;
}
export function allDrivers(): EngineDriver[] {
  return driver ? [driver] : [];
}
