// Settings tab: the diagnosis, Claude auth, tvmcp, the day/risk files,
// model/effort, permissions, theme, the «Справка» section (version, updates,
// links, changelog), trader profile.
import type { ComponentChildren } from 'preact';
import { useEffect, useState } from 'preact/hooks';
import type { ChatKind, EffortLevel, SecretName, Settings, SettingsPatch, Status, TraderProfile, UpdateInstallResult, UpdateState, WorkflowDef } from '@shared/api';
import { APP_LINKS } from '@shared/api';
import type { PreflightFix, PreflightResult } from '@shared/preflight';
import type { TaskId, TaskState } from '@shared/tasks';
import { ZOOM_STEPS } from '@shared/zoom';
import { t } from '../i18n';
import { limitationKeys } from '../setup';
import { SHELF_KINDS } from '../state';
import { GuideView } from './GuideView';
import { PreflightList } from './PreflightList';
// The changelog ships inside the bundle (Vite inlines `?raw` at build time), the
// same way guide.ts carries README.md — so no electron-builder `files:` entry.
import changelogMd from '../../../CHANGELOG.md?raw';

/** The title and the intro paragraph are for GitHub; inside the <details> the summary already says it. */
const CHANGELOG_BODY = changelogMd.slice(Math.max(0, changelogMd.indexOf('\n## ')));

const ruDate = (iso: string): string => {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleDateString('ru-RU', { day: '2-digit', month: '2-digit', year: 'numeric' });
};
const ruTime = (ms: number): string => new Date(ms).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' });
const mb = (bytes: number | undefined): string => ((bytes ?? 0) / 1048576).toFixed(1);

export interface SettingsTabProps {
  settings: Settings;
  profile: TraderProfile;
  status: Status | null;
  workflows: WorkflowDef[];
  /** The last diagnosis; null until the user asks for one. */
  preflight: PreflightResult | null;
  preflightBusy: boolean;
  onPreflight: () => void;
  onFix: (fix: PreflightFix) => void;
  /** Live state and the command of each long operation the diagnosis can start. */
  tasks?: Partial<Record<TaskId, TaskState>>;
  taskCommands?: Partial<Record<TaskId, string>>;
  onTaskStop?: (id: TaskId) => void;
  onTaskInput?: (id: TaskId, text: string) => void;
  onOpenWizard: () => void;
  /** Delete the provisioned Python environment; absent in a dev run, where there is none. */
  onClearRuntime?: () => void;
  onSettings: (patch: SettingsPatch) => void;
  /** System notice in the corner of the window (saved, failed, launched). */
  onToast: (text: string, level?: 'info' | 'warn' | 'error') => void;
  onSecret: (name: SecretName, value: string | null) => void;
  onProfile: (p: TraderProfile) => void;
  /** Native folder picker for the vault; the result lands in settings.vaultPath. */
  onPickVault: () => void;
  /** The updater's state, owned by app.tsx so the banner under the tabs and this section agree. */
  update: UpdateState | null;
  onUpdateCheck: () => void;
  onUpdateDownload: () => void;
  /** Resolves with the refusal, if any; the toast for it is app.tsx's, shared with the banner. */
  onUpdateInstall: () => Promise<UpdateInstallResult>;
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

const openExternal = (url: string) => void window.desk.open.external(url);

/** Настройки → «Справка»: the version, the update flow, the secret the private repo needs, the links, the changelog. */
function AboutSection(p: Pick<SettingsTabProps, 'settings' | 'update' | 'onSettings' | 'onSecret' | 'onUpdateCheck' | 'onUpdateDownload' | 'onUpdateInstall'>) {
  const [version, setVersion] = useState<string>('');
  useEffect(() => {
    void window.desk.app
      .version()
      .then(setVersion)
      .catch(() => undefined);
  }, []);
  const u = p.update;
  const status = u?.status ?? 'idle';
  const busy = status === 'checking' || status === 'downloading';
  const shown = version || u?.current || '';
  const notesUrl = u?.url || APP_LINKS.releases;

  let line: ComponentChildren;
  switch (status) {
    case 'dev':
      line = t('about.statusDev');
      break;
    case 'checking':
      line = t('about.statusChecking');
      break;
    case 'none':
      line = t('about.statusNone', { time: ruTime(u?.checkedAt ?? Date.now()) });
      break;
    case 'available':
      line = (
        <>
          <span>{u?.date ? t('about.statusAvailable', { v: u.version ?? '', date: ruDate(u.date) }) : t('about.statusAvailableNoDate', { v: u?.version ?? '' })}</span>
          <button type="button" class="primary" onClick={p.onUpdateDownload}>
            {t('about.download')}
          </button>
          <a href={notesUrl} onClick={(e) => (e.preventDefault(), openExternal(notesUrl))}>
            {t('about.notes')}
          </a>
        </>
      );
      break;
    case 'downloading': {
      const percent = Math.max(0, Math.min(100, Math.round(u?.percent ?? 0)));
      line = (
        <>
          <span>{t('about.statusDownloading', { percent, mb: mb(u?.bytes), mbTotal: mb(u?.total) })}</span>
          <div class="update-progress" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent}>
            <div class="update-progress-bar" style={{ width: `${percent}%` }} />
          </div>
        </>
      );
      break;
    }
    case 'downloaded':
      line = (
        <>
          <span>{t('about.statusDownloaded', { v: u?.version ?? '' })}</span>
          <button type="button" class="primary" onClick={() => void p.onUpdateInstall()}>
            {t('about.install')}
          </button>
        </>
      );
      break;
    case 'error':
      line = <span class="update-error">{t('about.statusError', { error: u?.error ?? '—' })}</span>;
      break;
    default:
      line = t('about.statusIdle');
  }

  return (
    <fieldset class="about">
      <legend>{t('about.legend')}</legend>
      <div class="about-heading">{t('about.heading')}</div>
      <div class="about-row">
        <span class="about-label">{t('about.version')}</span>
        <span class="about-version">{shown ? t('about.versionValue', { v: shown }) : '…'}</span>
        <button type="button" disabled={busy || status === 'dev'} onClick={p.onUpdateCheck}>
          {status === 'checking' ? t('about.checking') : t('about.check')}
        </button>
      </div>
      <div class={`about-status ${status}`}>{line}</div>
      <label class="check">
        <input type="checkbox" checked={p.settings.autoCheckUpdates !== false} onChange={(e) => p.onSettings({ autoCheckUpdates: (e.target as HTMLInputElement).checked })} />
        <span>{t('about.autoCheck')}</span>
      </label>
      <SecretField label={t('about.ghToken')} isSet={Boolean(p.settings.hasGhToken)} onSave={(v) => p.onSecret('ghToken', v)} onClear={() => p.onSecret('ghToken', null)} />
      <p class="muted">{t('about.ghTokenHint')}</p>
      <div class="btn-row">
        <button type="button" onClick={() => openExternal(APP_LINKS.issues)}>
          {t('about.reportBug')}
        </button>
        <button type="button" onClick={() => openExternal(APP_LINKS.repo)}>
          {t('about.website')}
        </button>
      </div>
      <p class="muted">{t('about.linksHint')}</p>
      <details class="changelog">
        <summary>{t('about.changelog')}</summary>
        <GuideView md={CHANGELOG_BODY} images={{}} />
      </details>
    </fieldset>
  );
}

export function SettingsTab(p: SettingsTabProps) {
  const s = p.settings;
  const [profile, setProfile] = useState<TraderProfile>(p.profile);
  const [checklist, setChecklist] = useState(p.profile.morningChecklist.join('\n'));
  const [text, setText] = useState({
    tvmcpPath: s.tvmcpPath,
    gtRagPath: s.gtRagPath ?? '',
    cdpPort: String(s.cdpPort),
    toolsets: s.toolsets,
    model: s.model,
    codexPath: s.codexPath ?? '',
    tvDesktopExe: s.tvDesktopExe,
    riskRulesPath: s.riskRulesPath,
    journalPath: s.journalPath,
    fieldNotesPath: s.fieldNotesPath,
    vaultPath: s.vaultPath ?? '',
  });

  useEffect(() => {
    setProfile(p.profile);
    setChecklist(p.profile.morningChecklist.join('\n'));
  }, [p.profile]);

  useEffect(() => {
    setText({
      tvmcpPath: s.tvmcpPath,
      gtRagPath: s.gtRagPath ?? '',
      cdpPort: String(s.cdpPort),
      toolsets: s.toolsets,
      model: s.model,
      codexPath: s.codexPath ?? '',
      tvDesktopExe: s.tvDesktopExe,
      riskRulesPath: s.riskRulesPath,
      journalPath: s.journalPath,
      fieldNotesPath: s.fieldNotesPath,
      vaultPath: s.vaultPath ?? '',
    });
  }, [s.tvmcpPath, s.gtRagPath, s.cdpPort, s.toolsets, s.model, s.codexPath, s.tvDesktopExe, s.riskRulesPath, s.journalPath, s.fieldNotesPath, s.vaultPath]);

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

  /** A path with a native picker next to it, and the check that says whether it is right. */
  const vaultStatus = p.preflight?.checks.find((c) => c.id === 'vault');
  const pathField = (key: 'vaultPath', label: string, hint: string, onPick: () => void) => (
    <label class="field">
      <span>{label}</span>
      <div class="inline">
        <input value={text[key]} onInput={(e) => setText({ ...text, [key]: (e.target as HTMLInputElement).value })} onBlur={() => commitText(key)} onKeyDown={(e) => e.key === 'Enter' && commitText(key)} />
        <button type="button" onClick={onPick}>
          {t('preflight.fix.pickDir')}
        </button>
      </div>
      <small>{hint}</small>
      {vaultStatus ? <small class={`pf-inline ${vaultStatus.status}`}>{t(vaultStatus.detailKey, vaultStatus.vars)}</small> : null}
    </label>
  );

  const saveProfile = () => {
    const next = { ...profile, morningChecklist: checklist.split('\n').map((l) => l.trim()).filter(Boolean) };
    p.onProfile(next);
    p.onToast(t('settings.saved'));
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
        <legend>{t('settings.macro')}</legend>
        {pathField('vaultPath', t('settings.vaultPath'), t('settings.vaultHint'), p.onPickVault)}
        <SecretField label={t('settings.tvSession')} isSet={s.hasTvSession} onSave={(v) => p.onSecret('tvSession', v)} onClear={() => p.onSecret('tvSession', null)} />
        <p class="muted">{t('settings.tvSessionHint')}</p>
        <label class="check">
          <input type="checkbox" checked={(s.macroMcp ?? 'bundled') === 'bundled'} onChange={(e) => p.onSettings({ macroMcp: (e.target as HTMLInputElement).checked ? 'bundled' : 'user' })} />
          <span>{t('settings.macroMcp')}</span>
        </label>
        <p class="muted">{t('settings.macroMcpHint')}</p>
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
        <label class="field">
          <span>{t('settings.uiScale')}</span>
          <select value={String(s.uiScale ?? 1)} onChange={(e) => p.onSettings({ uiScale: Number((e.target as HTMLSelectElement).value) })}>
            {ZOOM_STEPS.map((z) => (
              <option key={z} value={String(z)}>
                {Math.round(z * 100)}%
              </option>
            ))}
          </select>
        </label>
        <div class="muted">{t('settings.uiScaleHint')}</div>
        <label class="field">
          <span>{t('settings.answerLang')}</span>
          <select value={s.answerLang ?? 'ru'} onChange={(e) => p.onSettings({ answerLang: (e.target as HTMLSelectElement).value as Settings['answerLang'] })}>
            <option value="ru">{t('settings.answerLangRu')}</option>
            <option value="en">{t('settings.answerLangEn')}</option>
            <option value="auto">{t('settings.answerLangAuto')}</option>
          </select>
        </label>
        <div class="muted">{t('settings.answerLangHint')}</div>
      </fieldset>

      <AboutSection settings={s} update={p.update} onSettings={p.onSettings} onSecret={p.onSecret} onUpdateCheck={p.onUpdateCheck} onUpdateDownload={p.onUpdateDownload} onUpdateInstall={p.onUpdateInstall} />

      <details class="advanced" open={s.legacyUi}>
        <summary>{t('settings.advanced')}</summary>
        <p class="muted">{t('settings.advancedHint')}</p>
        <fieldset>
          <legend>{t('settings.legacyTitle')}</legend>
        <label class="check">
          <input type="checkbox" checked={Boolean(s.legacyUi)} onChange={(e) => p.onSettings({ legacyUi: (e.target as HTMLInputElement).checked })} />
          <span>{t('settings.legacyUi')}</span>
        </label>
        </fieldset>

      <fieldset>
        <legend>{t('preflight.title')}</legend>
        <div class="muted">{t('preflight.intro')}</div>
        <PreflightList
          result={p.preflight}
          busy={p.preflightBusy}
          tasks={p.tasks}
          taskCommands={p.taskCommands}
          onRecheck={p.onPreflight}
          onFix={p.onFix}
          onTaskStop={p.onTaskStop}
          onTaskInput={p.onTaskInput}
        />
        {limitationKeys(s).map((key) => (
          <p class="notice warn" key={key}>
            {t(key)}
          </p>
        ))}
        <div class="btn-row">
          <button type="button" onClick={p.onOpenWizard}>
            {t('setup.open')}
          </button>
          {p.onClearRuntime ? (
            <button type="button" onClick={p.onClearRuntime}>
              {t('settings.clearRuntime')}
            </button>
          ) : null}
        </div>
        {p.onClearRuntime ? <p class="muted">{t('settings.clearRuntimeNote')}</p> : null}
      </fieldset>

      <fieldset>
        <legend>{t('settings.engine')}</legend>
        <label class="field">
          <span>{t('settings.engineWhich')}</span>
          <select value={s.engine ?? 'claude'} onChange={(e) => p.onSettings({ engine: (e.target as HTMLSelectElement).value as Settings['engine'] })}>
            <option value="claude">{t('settings.engineClaude')}</option>
            <option value="codex">{t('settings.engineCodex')}</option>
          </select>
        </label>
        <small>{t('settings.engineHint')}</small>
        {(s.engine ?? 'claude') === 'codex' ? (
          <>
            {textField('codexPath', t('settings.codexPath'), t('settings.codexPathHint'))}
            <p class="muted">{t('settings.codexNote')}</p>
          </>
        ) : null}
      </fieldset>

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
        <legend>{t('settings.knowledge')}</legend>
        <label class="check">
          <input type="checkbox" checked={s.knowledgeBase !== false} onChange={(e) => p.onSettings({ knowledgeBase: (e.target as HTMLInputElement).checked })} />
          <span>{t('settings.knowledgeBase')}</span>
        </label>
        {textField('gtRagPath', t('settings.gtRagPath'), t('settings.gtRagHint'))}
      </fieldset>


      {s.legacyUi ? (
      <fieldset>
        <legend>{t('settings.dayDesk')}</legend>
        {textField('riskRulesPath', t('settings.riskRulesPath'), t('settings.riskRulesHint'))}
        {textField('journalPath', t('settings.journalPath'), t('settings.journalHint'))}
        {textField('fieldNotesPath', t('settings.fieldNotesPath'), t('settings.fieldNotesHint'))}
        <label class="field">
          <span>{t('settings.shelfWorkflows')}</span>
          <div class="shelf-map">
            {SHELF_KINDS.filter((k) => k !== 'macro').map((k: ChatKind) => (
              <label class="inline" key={k}>
                <span class="shelf-name">{t(`shelf.${k}`)}</span>
                <select
                  value={s.shelfWorkflows?.[k] ?? ''}
                  onChange={(e) => {
                    const v = (e.target as HTMLSelectElement).value;
                    const next = { ...(s.shelfWorkflows ?? {}) };
                    if (v) next[k] = v;
                    else delete next[k];
                    p.onSettings({ shelfWorkflows: next });
                  }}
                >
                  <option value="">{t('settings.shelfDefault')}</option>
                  {p.workflows.map((w) => (
                    <option value={w.id} key={w.id}>
                      {w.title}
                    </option>
                  ))}
                </select>
              </label>
            ))}
          </div>
          <small>{t('settings.shelfWorkflowsHint')}</small>
        </label>
      </fieldset>
      ) : null}

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
        </div>
      </fieldset>
      </details>
    </section>
  );
}
