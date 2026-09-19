import { ru, type Dict } from './ru';
import { en } from './en';

export type Lang = 'ru' | 'en';
let current: Lang = 'ru';

export function setLang(lang: Lang): void {
  current = lang;
}

function lookup(dict: unknown, path: string[]): unknown {
  let node: unknown = dict;
  for (const p of path) {
    if (!node || typeof node !== 'object' || !(p in (node as object))) return undefined;
    node = (node as Record<string, unknown>)[p];
  }
  return node;
}

/**
 * t('settings.title') → string; t('notice.mcp', {name, status}) interpolates
 * `{name}` placeholders. Unknown keys return the key itself.
 */
export function t(key: string, vars?: Record<string, string | number>): string {
  const path = key.split('.');
  const value = (current === 'en' ? lookup(en, path) : undefined) ?? lookup(ru, path);
  let s = typeof value === 'string' ? value : key;
  if (vars) for (const [k, v] of Object.entries(vars)) s = s.split(`{${k}}`).join(String(v));
  return s;
}

export type { Dict };
