// Card dispatcher: one parsed fenced block → its native card.
import type { Block } from '@shared/blocks';
import type { Segment } from '../chat/blocks';
import { BriefReportView } from './BriefReport';
import { ChartCardView } from './ChartCard';
import { NextStepsCard } from './NextSteps';
import { ErrorCard, HealthDataCard, StreamingCard } from './Placeholder';
import { ProdataCard } from './Prodata';
import { StressReportView } from './StressReport';
import { WorkflowStateView } from './WorkflowState';

export { CardActionsContext, type CardActions } from './context';
export { GradeBadge, StressReportView } from './StressReport';

export function BlockView({ block }: { block: Block }) {
  switch (block.kind) {
    case 'nextsteps':
      return <NextStepsCard data={block.data} />;
    case 'chartcard':
      return <ChartCardView data={block.data} />;
    case 'stressreport':
      return <StressReportView data={block.data} />;
    case 'prodata':
      return <ProdataCard data={block.data} />;
    case 'briefreport':
      return <BriefReportView data={block.data} />;
    case 'healthdata':
      return <HealthDataCard data={block.data} />;
    case 'workflowstate':
      return <WorkflowStateView data={block.data} />;
  }
}

export function CardSegment({ seg }: { seg: Extract<Segment, { type: 'card' }> }) {
  if (seg.open || !seg.parsed) return <StreamingCard kind={seg.lang} />;
  if (!seg.parsed.ok) return <ErrorCard kind={seg.lang} error={seg.parsed.error} raw={seg.raw} />;
  return <BlockView block={seg.parsed.block} />;
}
