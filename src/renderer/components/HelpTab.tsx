// Static help: connection diagram, three steps, troubleshooting — plus the one
// button that turns "it does not work" into a list of next steps.
import { t } from '../i18n';
import { usageGuide } from '../guide';
import { GuideView } from './GuideView';

export interface HelpTabProps {
  onOpenWizard: () => void;
  /** The macro-review MVP: the usage part of docs/GUIDE.md, nothing about plumbing. */
  simple?: boolean;
}

/** The full guide (installation, animations) lives in the repository; the tab shows sections 5–14. */
const GUIDE_URL = 'https://github.com/Miha21222/strategy-desk/blob/main/docs/GUIDE.md';

export function HelpTab(p: HelpTabProps) {
  if (p.simple) {
    const guide = usageGuide();
    return (
      <section class="page help-simple">
        <h2>{t('helpSimple.title')}</h2>
        <div class="help-wizard">
          <p>{t('help.wizard')}</p>
          <button type="button" onClick={p.onOpenWizard}>
            {t('setup.open')}
          </button>
        </div>
        <p>
          {t('helpSimple.intro')}{' '}
          <a href={GUIDE_URL} onClick={(e) => (e.preventDefault(), void window.desk.open.external(GUIDE_URL))}>
            {t('helpSimple.fullGuide')}
          </a>
        </p>
        <GuideView md={guide.md} images={guide.images} />
      </section>
    );
  }
  return (
    <section class="page">
      <h2>{t('help.title')}</h2>

      <div class="help-wizard">
        <p>{t('help.wizard')}</p>
        <button type="button" class="primary" onClick={p.onOpenWizard}>
          {t('setup.open')}
        </button>
      </div>

      <h3>{t('help.howTitle')}</h3>
      <div class="diagram">
        <div class="box">{t('help.boxApp')}</div>
        <div class="arrow">→</div>
        <div class="box">{t('help.boxSdk')}</div>
        <div class="arrow">→</div>
        <div class="box">{t('help.boxTvmcp')}</div>
        <div class="arrow">→</div>
        <div class="box">
          {t('help.boxTv')}
          <small>{t('help.boxTvNote')}</small>
        </div>
      </div>

      <h3>{t('help.stepsTitle')}</h3>
      <ol>
        <li>{t('help.step1')}</li>
        <li>{t('help.step2')}</li>
        <li>{t('help.step3')}</li>
      </ol>

      <h3>{t('help.dayTitle')}</h3>
      <ul>
        <li>{t('help.day1')}</li>
        <li>{t('help.day2')}</li>
        <li>{t('help.day3')}</li>
      </ul>

      <h3>{t('help.troubleTitle')}</h3>
      <ul>
        <li>{t('help.t1')}</li>
        <li>{t('help.t2')}</li>
        <li>{t('help.t3')}</li>
        <li>{t('help.t4')}</li>
        <li>{t('help.t5')}</li>
      </ul>
    </section>
  );
}
