// Stand-ins for modules startChat imports but the macro / ask path does not use.
import { readFileSync } from 'node:fs';
import path from 'node:path';

const ROOT = process.env.CHAT_FOLD_REPO || process.cwd();
/** resources.ts: the shipped prompt files, read from the repo like a dev run. */
export async function readResource(...parts: string[]): Promise<string | null> {
  try {
    return readFileSync(path.join(ROOT, ...parts), 'utf8');
  } catch {
    return null;
  }
}
export function binaryCandidates(): string[] {
  return [];
}
export async function taskInFlight(): Promise<void> {}
export async function getWorkflow(): Promise<null> {
  return null;
}
export async function promptForChat(): Promise<string> {
  return '';
}
export function assemblePrompt(): string {
  return '';
}
export function buildMcpServers(): Record<string, never> {
  return {};
}
// session.ts: never reached — the fake driver replays the stream itself.
export async function acquire(): Promise<never> {
  throw new Error('session stub');
}
export const closeAll = (): void => {};
export const closeFor = (): void => {};
export const fingerprintOptions = (): string => 'stub';
export const hasLive = (): boolean => false;
export async function prewarm(): Promise<void> {}
export async function runTurn(): Promise<void> {}
