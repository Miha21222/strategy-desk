// The one-line notice under the tabs when a newer release exists or is already
// downloaded. A toast lasts 2.6 s, which is not a notification; this stays until
// the user acts on it or dismisses THAT version (remembered in localStorage, so
// the same release does not nag on every start — a newer one shows again).
import { useState } from 'preact/hooks';
import type { UpdateInstallResult, UpdateState } from '@shared/api';
import { t } from '../i18n';

export interface UpdateBannerProps {
  update: UpdateState | null;
  onDownload: () => void;
  onInstall: () => Promise<UpdateInstallResult>;
  /** «Подробнее»: the Settings tab, where the «Справка» section has the whole flow. */
  onMore: () => void;
}

const DISMISSED_KEY = 'update.dismissed';

function readDismissed(): string {
  try {
    return localStorage.getItem(DISMISSED_KEY) ?? '';
  } catch {
    return '';
  }
}

export function UpdateBanner(p: UpdateBannerProps) {
  const [dismissed, setDismissed] = useState<string>(readDismissed);
  const u = p.update;
  if (!u || !u.version) return null;
  if (u.status !== 'available' && u.status !== 'downloaded') return null;
  if (dismissed === u.version) return null;

  const dismiss = () => {
    setDismissed(u.version ?? '');
    try {
      localStorage.setItem(DISMISSED_KEY, u.version ?? '');
    } catch {
      /* a private window: the banner only comes back on the next start */
    }
  };

  const downloaded = u.status === 'downloaded';
  return (
    <div class={`update-banner ${downloaded ? 'ready' : ''}`} role="status">
      <span class="update-banner-text">{downloaded ? t('about.bannerDownloaded', { v: u.version }) : t('about.bannerAvailable', { v: u.version })}</span>
      {downloaded ? (
        <button type="button" class="primary" onClick={() => void p.onInstall()}>
          {t('about.install')}
        </button>
      ) : (
        <button type="button" class="primary" onClick={p.onDownload}>
          {t('about.download')}
        </button>
      )}
      <button type="button" class="link" onClick={p.onMore}>
        {t('about.bannerMore')}
      </button>
      <button type="button" class="update-banner-close" title={t('about.bannerDismiss')} aria-label={t('about.bannerDismiss')} onClick={dismiss}>
        ×
      </button>
    </div>
  );
}
