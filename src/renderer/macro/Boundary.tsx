// A render error in the review must not take the whole window down: it shows
// a short note with a retry, and the rest of the app (chat, settings) keeps
// working. The error itself goes to the console, where `npm run dev` prints it.
//
// A function component on purpose: a class boundary is not hot-swappable by
// Preact's fast refresh, and during `npm run dev` an edit then left the old
// review mounted next to the new one (the tab appeared twice).
import type { ComponentChildren } from 'preact';
import { useErrorBoundary } from 'preact/hooks';
import { t } from '../i18n';

export function MacroBoundary({ children }: { children: ComponentChildren }) {
  const [error, reset] = useErrorBoundary((e) => console.error('[macro] render failed:', e));
  if (!error) return <>{children}</>;
  return (
    <section class="macro">
      <div class="macro-none">
        <p>{t('macro.crashed')}</p>
        <p class="muted small">{String((error as Error)?.message ?? error)}</p>
        <button type="button" onClick={reset}>
          {t('macro.retry')}
        </button>
      </div>
    </section>
  );
}
