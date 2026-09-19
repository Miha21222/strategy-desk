// Settings tab: Claude auth, tvmcp, model/effort, permissions, theme, trader profile.
import { useEffect, useState } from 'preact/hooks';
import type { EffortLevel, Settings, SettingsPatch, Status, TraderProfile } from '@shared/api';
import { t } from '../i18n';

export interface SettingsTabProps {
  settings: Settings;
  profile: TraderProfile;
  status: Status | null;
  onSettings: (patch: SettingsPatch) => void;
  onSecret: (name: 'apiKey' | 'oauthToken', value: string | null) => void;
  onProfile: (p: TraderProfile) => void;
}

const EFFORTS: EffortLevel[] = ['', 'low', 'medium', 'high', 'xhigh', 'max'];

function SecretField({ label, isSet, onSave, onClear }: { label: string; isSet: boolean; onSave: (v: string) => void; onClear: () => void }) {
  const [value, setValue] = useState('');
  return (
    <label class="field">
      <span>
        {label} — <em>{isSet ? t('settings.secretSet') : t('settings.secretNotSet')}</em>
      </span>
      <div class="inline">
        <input type="password" value={value} autocomplete="off" onInput={(e) => setValue((e.target as HTMLInputElement).value)} />
        <button
          type="button"
          class="primary"
          disabled={!value.trim()}
          onClick={() => {
            onSave(value.trim());
            setValue('');
          }}
        >
          {t('settings.saveSecret')}
        </button>
        <button type="button" disabled={!isSet} onClick={onClear}>
          {t('settings.clearSecret')}
        </button>
      </div>
      <small>{t('settings.secretHint')}</small>
    </label>
  );
}

export function SettingsTab(p: SettingsTabProps) {
  const s = p.settings;
  const [profile, setProfile] = useState<TraderProfile>(p.profile);
  const [checklist, setChecklist] = useState(p.profile.morningChecklist.join('\n'));
  const [savedAt, setSavedAt] = useState(0);
  const [text, setText] = useState({ tvmcpPath: s.tvmcpPath, cdpPort: String(s.cdpPort), toolsets: s.toolsets, model: s.model, tvDesktopExe: s.tvDesktopExe });

  useEffect(() => {
    setProfile(p.profile);
    setChecklist(p.profile.morningChecklist.join('\n'));
  }, [p.profile]);

  useEffect(() => {
    setText({ tvmcpPath: s.tvmcpPath, cdpPort: String(s.cdpPort), toolsets: s.toolsets, model: s.model, tvDesktopExe: s.tvDesktopExe });
  }, [s.tvmcpPath, s.cdpPort, s.toolsets, s.model, s.tvDesktopExe]);

  const commitText = (key: keyof typeof text) => {
    const v = text[key].trim();
    if (key === 'cdpPort') {
      const n = Number(v);
      if (Number.isInteger(n) && n > 0 && n < 65536 && n !== s.cdpPort) p.onSettings({ cdpPort: n });
      else setText({ ...text, cdpPort: String(s.cdpPort) });
      return;
    }
    if (v !== s[key]) p.onSettings({ [key]: v });
  };

  const textField = (key: keyof typeof text, label: string, hint?: string) => (
    <label class="field">
      <span>{label}</span>
      <input value={text[key]} onInput={(e) => setText({ ...text, [key]: (e.target as HTMLInputElement).value })} onBlur={() => commitText(key)} onKeyDown={(e) => e.key === 'Enter' && commitText(key)} />
      {hint ? <small>{hint}</small> : null}
    </label>
  );

  const saveProfile = () => {
    const next = { ...profile, morningChecklist: checklist.split('\n').map((l) => l.trim()).filter(Boolean) };
    p.onProfile(next);
    setSavedAt(Date.now());
    setTimeout(() => setSavedAt(0), 1500);
  };

  const pf = (key: keyof Omit<TraderProfile, 'morningChecklist'>, label: string) => (
    <label class="field">
      <span>{label}</span>
      <input value={profile[key]} onInput={(e) => setProfile({ ...profile, [key]: (e.target as HTMLInputElement).value })} />
    </label>
  );

  return (
    <section class="page">
      <h2>{t('settings.title')}</h2>

      <fieldset>
        <legend>{t('settings.claude')}</legend>
        <div class="muted">
          {t('settings.currentAuth')}: {p.status ? t(`claude.${p.status.claude}`) : '…'}
        </div>
        <label class="field">
          <span>{t('settings.authMode')}</span>
          <select value={s.authMode} onChange={(e) => p.onSettings({ authMode: (e.target as HTMLSelectElement).value as Settings['authMode'] })}>
            <option value="subscription">{t('settings.authSubscription')}</option>
            <option value="api-key">{t('settings.authApiKey')}</option>
            <option value="setup-token">{t('settings.authSetupToken')}</option>
          </select>
        </label>
        <SecretField label={t('settings.apiKey')} isSet={s.hasApiKey} onSave={(v) => p.onSecret('apiKey', v)} onClear={() => p.onSecret('apiKey', null)} />
        <SecretField label={t('settings.oauthToken')} isSet={s.hasOauthToken} onSave={(v) => p.onSecret('oauthToken', v)} onClear={() => p.onSecret('oauthToken', null)} />
      </fieldset>

      <fieldset>
        <legend>{t('settings.tvmcp')}</legend>
        {textField('tvmcpPath', t('settings.tvmcpPath'))}
        {textField('cdpPort', t('settings.cdpPort'))}
        {textField('toolsets', t('settings.toolsets'))}
        {textField('tvDesktopExe', t('settings.tvExe'))}
        <label class="check">
          <input type="checkbox" checked={s.officialMcp} onChange={(e) => p.onSettings({ officialMcp: (e.target as HTMLInputElement).checked })} />
          <span>{t('settings.officialMcp')}</span>
        </label>
      </fieldset>

      <fieldset>
        <legend>{t('settings.modelSection')}</legend>
        {textField('model', t('settings.model'))}
        <label class="field">
          <span>{t('settings.effort')}</span>
          <select value={s.effort} onChange={(e) => p.onSettings({ effort: (e.target as HTMLSelectElement).value as EffortLevel })}>
            {EFFORTS.map((e) => (
              <option value={e}>{e || t('settings.effortDefault')}</option>
            ))}
          </select>
        </label>
        <label class="field">
          <span>{t('settings.permission')}</span>
          <select value={s.permissionPreset} onChange={(e) => p.onSettings({ permissionPreset: (e.target as HTMLSelectElement).value as Settings['permissionPreset'] })}>
            <option value="standard">{t('settings.permissionStandard')}</option>
            <option value="extended">{t('settings.permissionExtended')}</option>
          </select>
        </label>
      </fieldset>

      <fieldset>
        <legend>{t('settings.appearance')}</legend>
        <label class="field">
          <span>{t('settings.theme')}</span>
          <select value={s.theme} onChange={(e) => p.onSettings({ theme: (e.target as HTMLSelectElement).value as Settings['theme'] })}>
            <option value="system">{t('settings.themeSystem')}</option>
            <option value="dark">{t('settings.themeDark')}</option>
            <option value="light">{t('settings.themeLight')}</option>
          </select>
        </label>
      </fieldset>

      <fieldset>
        <legend>{t('settings.profile')}</legend>
        <div class="muted">{t('settings.profileHint')}</div>
        {pf('callName', t('settings.callName'))}
        {pf('traderType', t('settings.traderType'))}
        {pf('markets', t('settings.markets'))}
        {pf('style', t('settings.style'))}
        {pf('riskRules', t('settings.riskRules'))}
        {pf('propFirms', t('settings.propFirms'))}
        <label class="field">
          <span>{t('settings.morningChecklist')}</span>
          <textarea rows={5} value={checklist} onInput={(e) => setChecklist((e.target as HTMLTextAreaElement).value)} />
        </label>
        <div class="btn-row">
          <button type="button" class="primary" onClick={saveProfile}>
            {t('settings.save')}
          </button>
          {savedAt ? <span class="muted">{t('settings.saved')}</span> : null}
        </div>
      </fieldset>
    </section>
  );
}
