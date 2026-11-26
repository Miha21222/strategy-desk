// src/main/runtime.ts minus Electron: no payload, no venv.
export function claudeBin(): string {
  return '';
}
export async function initRuntimePaths(): Promise<{ venvDir: string | null }> {
  return { venvDir: null };
}
