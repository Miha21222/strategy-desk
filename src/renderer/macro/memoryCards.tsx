// The memory cards of note 23 §2–3: «Сегодня помни», «Что изменилось»,
// «Неделя в пять строк», «Идеи на проверке».
//
// The desk is a memory prosthesis, not a forecast screen: the skill decides
// which few things of its whole memory matter today and writes them to small
// state files; these cards only show them. Every card is read-only and SILENT
// when its file is missing or has nothing to say — no empty box, no «пока
// пусто». One thought per window, at most three lines, plain Russian, no
// buttons: a decision (a rule, a share) is the owner's and is made in chat.
import type { Drift, Idea, Ideas, Remember, WeekSummary } from '@shared/macro';
import { visibleIdeas } from '@shared/macro';
import { t } from '../i18n';

/** «1 раз», «3 раза», «5 раз». */
function times(n: number): string {
  const m10 = n % 10;
  const m100 = n % 100;
  if (m10 === 1 && m100 !== 11) return t('macro.timesOne', { n });
  if (m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14)) return t('macro.timesFew', { n });
  return t('macro.timesMany', { n });
}

/**
 * «Сегодня помни» — up to three lines picked for this window: a lesson whose
 * conditions match today, a rule of note 20 that is on today, a similar day
 * and how it ended. Sits right under the owner's read, above the gauge.
 */
export function RememberCard({ remember }: { remember: Remember | null | undefined }) {
  if (!remember?.items.length) return null;
  return (
    <div class="card memo remember">
      <div class="card-head">{t('macro.rememberTitle')}</div>
      <div class="card-body">
        <ul class="memo-list">
          {remember.items.map((it) => (
            <li class={`memo-item kind-${it.kind}`}>
              <span class="memo-kind">{t(`macro.rememberKind.${it.kind}`)}</span>
              <span class="memo-main">
                <span class="memo-text">
                  {it.text}
                  {it.ref ? <small class="memo-ref muted"> · {it.ref}</small> : null}
                </span>
                {it.why ? <span class="memo-why muted small">{it.why}</span> : null}
              </span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

/**
 * «Что изменилось» — a slim strip, one line per signal, only when there is
 * one: the phase turned, the ADR shrank, a factor that used to guess has been
 * missing for days. A warning keeps the warning tint.
 */
export function DriftStrip({ drift }: { drift: Drift | null | undefined }) {
  if (!drift?.signals.length) return null;
  return (
    <div class="card memo drift">
      <div class="card-head">{t('macro.driftTitle')}</div>
      <div class="card-body">
        {drift.signals.map((s) => (
          <div class={`drift-line sev-${s.severity}`}>
            <span class="drift-dot" aria-hidden="true" />
            <span>{s.text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

/** The share proposal as one sentence; no button — the owner answers in chat. */
function shareSentence(p: NonNullable<WeekSummary['share_proposal']>): string {
  const move = p.from != null && p.to != null ? t('macro.weekShareMove', { factor: p.factor, from: String(p.from), to: String(p.to) }) : t('macro.weekShareOnly', { factor: p.factor });
  return p.why ? `${move} ${p.why}` : move;
}

/** «О тебе»: what the skill noticed about the trader himself over the weeks. */
function TraderBlock({ lines }: { lines: string[] }) {
  if (!lines.length) return null;
  return (
    <div class="trader">
      <div class="sub">{t('macro.traderTitle')}</div>
      <ul class="memo-plain">
        {lines.map((x) => (
          <li>{x}</li>
        ))}
      </ul>
    </div>
  );
}

/**
 * «Неделя в пять строк» — the Friday evening: the market, what worked, the
 * lesson that repeated, one idea to check, one share proposal. `trader` is
 * the evening's «О тебе» block, shown under the lines when there is one.
 */
export function WeekCard({ week, trader }: { week: WeekSummary | null | undefined; trader: string[] }) {
  if (!week?.lines.length) return null;
  const range = week.week_start && week.week_end ? `${week.week_start.slice(8, 10)}.${week.week_start.slice(5, 7)} – ${week.week_end.slice(8, 10)}.${week.week_end.slice(5, 7)}` : '';
  return (
    <div class="card memo week wide">
      <div class="card-head">
        {t('macro.weekTitle')}
        {range ? <span class="muted"> · {range}</span> : null}
      </div>
      <div class="card-body">
        <ol class="week-lines">
          {week.lines.map((x) => (
            <li>{x}</li>
          ))}
        </ol>
        <TraderBlock lines={trader} />
        {week.share_proposal ? <p class="week-share muted">{shareSentence(week.share_proposal)}</p> : null}
      </div>
    </div>
  );
}

/** The evening's «О тебе» on a weekday (on a Friday it lives inside the week card). */
export function TraderCard({ lines }: { lines: string[] }) {
  if (!lines.length) return null;
  return (
    <div class="card memo">
      <div class="card-body">
        <TraderBlock lines={lines} />
      </div>
    </div>
  );
}

function IdeaRow({ idea }: { idea: Idea }) {
  return (
    <li class={`idea st-${idea.status}`}>
      <div class="idea-row">
        <span class="idea-text">{idea.text}</span>
        <span class="idea-score" title={t('macro.ideaScoreTip', { c: idea.confirmed, r: idea.refuted })}>
          <span class="pos">✓{idea.confirmed}</span> <span class="neg">✗{idea.refuted}</span>
        </span>
      </div>
      {idea.status === 'proposed' ? <div class="idea-proposed">{t('macro.ideaProposed', { times: times(idea.confirmed) })}</div> : null}
    </li>
  );
}

/**
 * «Идеи на проверке» — what the owner once said «а мне кажется…», with the
 * running count from the evening checks. Open and proposed ideas only, newest
 * first, at most five; a proposed one says so, and the decision is his.
 */
export function IdeasCard({ ideas }: { ideas: Ideas | null | undefined }) {
  const items = visibleIdeas(ideas);
  if (!items.length) return null;
  return (
    <div class="card memo ideas">
      <div class="card-head">{t('macro.ideasTitle')}</div>
      <div class="card-body">
        <ul class="idea-list">
          {items.map((x) => (
            <IdeaRow idea={x} />
          ))}
        </ul>
      </div>
    </div>
  );
}
