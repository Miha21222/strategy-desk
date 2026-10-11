// A render error in the review must not take the whole window down: it shows
// a short note with a retry, and the rest of the app (chat, settings) keeps
// working. The error itself goes to the console, where `npm run dev` prints it.
import { Component, type ComponentChildren } from 'preact';
import { t } from '../i18n';

export class MacroBoundary extends Component<{ children: ComponentChildren }, { error: string | null }> {
  state = { error: null as string | null };

  componentDidCatch(error: unknown) {
    console.error('[macro] render failed:', error);
    this.setState({ error: String((error as Error)?.message ?? error) });
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <section class="macro">
        <div class="macro-none">
          <p>{t('macro.crashed')}</p>
          <p class="muted small">{this.state.error}</p>
          <button type="button" onClick={() => this.setState({ error: null })}>
            {t('macro.retry')}
          </button>
        </div>
      </section>
    );
  }
}
