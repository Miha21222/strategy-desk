// The corner stack of system notices; the rules live in ../toasts.
import { t } from '../i18n';
import type { Toast } from '../toasts';

export interface ToastsProps {
  items: Toast[];
  onDismiss: (id: number) => void;
}

export function Toasts(p: ToastsProps) {
  if (!p.items.length) return null;
  return (
    <div class="toast-host" role="status" aria-live="polite">
      {p.items.map((toast) => (
        <div class={`toast ${toast.level}`} key={toast.id}>
          <span class="toast-text">{toast.text}</span>
          <button type="button" class="toast-close" title={t('toast.close')} aria-label={t('toast.close')} onClick={() => p.onDismiss(toast.id)}>
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
