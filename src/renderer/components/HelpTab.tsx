// Static help: connection diagram, three steps, troubleshooting — plus the one
// button that turns "it does not work" into a list of next steps.
import { t } from '../i18n';

export interface HelpTabProps {
  onOpenWizard: () => void;
  /** The macro-review MVP: explain the day review in plain words, nothing about plumbing. */
  simple?: boolean;
}

const SIMPLE_SECTIONS = ['what', 'when', 'read', 'argue', 'evening', 'trouble'] as const;

export function HelpTab(p: HelpTabProps) {
  if (p.simple) {
    return (
      <section class="page help-simple">
        <h2>{t('helpSimple.title')}</h2>
        {SIMPLE_SECTIONS.map((k) => (
          <div key={k}>
            <h3>{t(`helpSimple.${k}Title`)}</h3>
            <p>{t(`helpSimple.${k}`)}</p>
          </div>
        ))}
        <div class="help-wizard">
          <p>{t('help.wizard')}</p>
          <button type="button" onClick={p.onOpenWizard}>
            {t('setup.open')}
          </button>
        </div>
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
