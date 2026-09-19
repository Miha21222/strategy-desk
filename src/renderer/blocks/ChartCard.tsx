// chartcard: a titled stack of small charts (stats tiles, signed bars, donut,
// histogram, levels ladder) with a one-line note.
import type { Chart, ChartCard } from '@shared/blocks';
import { Donut, Hist, LevelsLadder, SignedBars, StatTiles } from './charts';

export function ChartView({ chart }: { chart: Chart }) {
  switch (chart.type) {
    case 'stats':
      return <StatTiles items={chart.items} />;
    case 'bars':
      return <SignedBars items={chart.items} format={chart.format} />;
    case 'donut':
      return <Donut pct={chart.pct} label={chart.label} sub={chart.sub} />;
    case 'hist':
      return <Hist bins={chart.bins} />;
    case 'levels':
      return <LevelsLadder items={chart.items} last={chart.last} />;
  }
}

export function ChartCardView({ data }: { data: ChartCard }) {
  return (
    <div class="card chartcard">
      <div class="card-head">{data.title}</div>
      <div class="card-body">
        {data.charts.map((chart, i) => (
          <section class={`chart chart-${chart.type}`} key={i}>
            {chart.label && chart.type !== 'donut' ? <div class="chart-label">{chart.label}</div> : null}
            <ChartView chart={chart} />
          </section>
        ))}
        {data.note ? <div class="card-note">{data.note}</div> : null}
      </div>
    </div>
  );
}
