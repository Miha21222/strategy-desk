// Tiny Pine Script tokenizer for syntax highlighting: comments, strings,
// numbers, keywords/types, builtin namespaces, annotations. Pure function,
// no DOM: returns tokens the CodeBlock maps to <span class="tok-…">.

export type PineTokenType = 'cmt' | 'str' | 'num' | 'kw' | 'type' | 'ns' | 'ann' | 'fn' | 'op' | 'text';
export interface PineToken {
  type: PineTokenType;
  text: string;
}

const KEYWORDS = new Set([
  'if', 'else', 'for', 'while', 'switch', 'and', 'or', 'not', 'var', 'varip', 'true', 'false', 'na',
  'import', 'export', 'method', 'type', 'enum', 'series', 'simple', 'const', 'in', 'to', 'by', 'return', 'as',
]);
const TYPES = new Set(['int', 'float', 'bool', 'string', 'color', 'label', 'line', 'box', 'table', 'array', 'matrix', 'map', 'linefill', 'polyline', 'chart', 'point']);
const NAMESPACES = new Set(['ta', 'math', 'str', 'strategy', 'input', 'request', 'ticker', 'syminfo', 'timeframe', 'barstate', 'session', 'color', 'array', 'matrix', 'map', 'label', 'line', 'box', 'table', 'plot', 'shape', 'location', 'size', 'xloc', 'yloc', 'extend', 'format', 'scale', 'display', 'alert', 'log', 'runtime', 'dividends', 'earnings', 'splits', 'currency', 'text', 'font', 'position', 'order', 'adjustment', 'backadjustment', 'settlement_as_close', 'hline', 'plotshape', 'plotchar', 'plotarrow', 'bgcolor', 'barcolor', 'fill', 'indicator', 'library', 'alertcondition', 'nz', 'fixnan', 'bar_index', 'open', 'high', 'low', 'close', 'volume', 'time', 'time_close', 'hl2', 'hlc3', 'ohlc4', 'hlcc4', 'last_bar_index', 'last_bar_time', 'dayofweek', 'dayofmonth', 'hour', 'minute', 'second', 'month', 'year', 'weekofyear', 'timenow', 'timestamp']);

const RE = /(\/\/.*)|("(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*')|(\b\d+(?:\.\d+)?(?:e[+-]?\d+)?\b)|(@\w+)|([A-Za-z_]\w*)|(:=|==|!=|<=|>=|=>|[-+*\/%<>=?:!])|(\s+|.)/g;

export function tokenizePine(src: string): PineToken[] {
  const out: PineToken[] = [];
  let m: RegExpExecArray | null;
  RE.lastIndex = 0;
  while ((m = RE.exec(src))) {
    const text = m[0];
    if (m[1]) out.push({ type: 'cmt', text });
    else if (m[2]) out.push({ type: 'str', text });
    else if (m[3]) out.push({ type: 'num', text });
    else if (m[4]) out.push({ type: 'ann', text });
    else if (m[5]) {
      const next = src[RE.lastIndex];
      if (KEYWORDS.has(text)) out.push({ type: 'kw', text });
      else if (TYPES.has(text) && next !== '.') out.push({ type: 'type', text });
      else if (NAMESPACES.has(text)) out.push({ type: 'ns', text });
      else if (next === '(') out.push({ type: 'fn', text });
      else out.push({ type: 'text', text });
    } else if (m[6]) out.push({ type: 'op', text });
    else {
      const last = out[out.length - 1];
      if (last && last.type === 'text') last.text += text;
      else out.push({ type: 'text', text });
    }
  }
  return out;
}

export function isPineLang(lang: string): boolean {
  const l = lang.toLowerCase();
  return l === 'pine' || l === 'pinescript' || l === 'pine-script';
}
