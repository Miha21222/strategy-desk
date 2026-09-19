// nextsteps card: chips. `prompt` sends as the user; `save_*` moves the chat
// to a shelf (+ renames) and flips to «Сохранено ✓»; `update_profile` appends
// to profile.style.
import { useContext, useState } from 'preact/hooks';
import type { ChatKind } from '@shared/api';
import type { NextSteps, NextStepsOption } from '@shared/blocks';
import { t } from '../i18n';
import { CardActionsContext } from './context';

const SAVE_KIND: Record<'save_strategy' | 'save_indicator' | 'save_performance', ChatKind> = {
  save_strategy: 'strategy',
  save_indicator: 'indicator',
  save_performance: 'performance',
};

function Chip({ opt }: { opt: NextStepsOption }) {
  const actions = useContext(CardActionsContext);
  const [done, setDone] = useState(false);
  const [pending, setPending] = useState(false);

  const run = async () => {
    if (pending || done) return;
    if ('prompt' in opt) {
      actions.send(opt.prompt);
      return;
    }
    setPending(true);
    try {
      if (opt.action === 'update_profile') await actions.appendProfileStyle(opt.text);
      else await actions.saveChat(SAVE_KIND[opt.action], opt.name);
      setDone(true);
    } finally {
      setPending(false);
    }
  };

  const isPrompt = 'prompt' in opt;
  const title = isPrompt ? opt.prompt : opt.action === 'update_profile' ? opt.text : (opt.name ?? '');
  return (
    <button type="button" class={`chip${isPrompt ? '' : ' action'}${done ? ' done' : ''}`} disabled={pending || done || (isPrompt && actions.busy)} onClick={() => void run()} title={title}>
      {done ? t('cards.saved') : opt.label}
    </button>
  );
}

export function NextStepsCard({ data }: { data: NextSteps }) {
  return (
    <div class="card nextsteps">
      <div class="card-head">{t('cards.nextSteps')}</div>
      <div class="chips">
        {data.options.map((opt, i) => (
          <Chip key={i} opt={opt} />
        ))}
      </div>
    </div>
  );
}
