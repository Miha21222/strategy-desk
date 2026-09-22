// Transient system notices: "saved", "TradingView launched", a failed launch.
// They used to be muted text next to the button that caused them, which is easy
// to miss; the stack lives in the corner of the window instead. Pure data here,
// the markup in components/Toasts.tsx, so the rules are testable on their own.

export type ToastLevel = 'info' | 'warn' | 'error';

export interface Toast {
  id: number;
  text: string;
  level: ToastLevel;
}

/** How long a toast stays up. A failure gets the time to be read and acted on. */
export const TOAST_MS: Record<ToastLevel, number> = { info: 2600, warn: 6000, error: 9000 };

/** At most this many at once; the oldest fall off the top of the stack. */
export const MAX_TOASTS = 4;

/**
 * Append a toast. Repeating the same text only refreshes the one already up
 * (clicking "save" twice must not stack two identical cards).
 */
export function addToast(list: Toast[], toast: Toast): Toast[] {
  const without = list.filter((x) => x.text !== toast.text);
  return [...without, toast].slice(-MAX_TOASTS);
}
