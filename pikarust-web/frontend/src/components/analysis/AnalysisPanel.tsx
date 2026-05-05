import type { AnalysisState } from '@/hooks/useAnalysis';
import { Panel } from '@/components/ui/Panel';

interface Props {
  analysis: AnalysisState;
}

export function AnalysisPanel({ analysis }: Props) {
  const { currentDepth, formattedScore, nodes, nps, pv, wdl } = analysis;

  return (
    <Panel title="Analysis">
      <div className="font-mono text-sm space-y-2">
        <div className="flex gap-4 text-[var(--color-text-dim)]">
          <span>d={currentDepth}</span>
          <span className="text-[var(--color-accent)] font-bold">{formattedScore}</span>
          <span>{nodes > 0 ? `${(nodes / 1000).toFixed(0)}k` : '—'}</span>
          <span>{nps > 0 ? `${(nps / 1000).toFixed(0)}kn/s` : ''}</span>
        </div>

        {wdl && <WdlBar wdl={wdl} />}

        {pv && (
          <div className="text-xs text-[var(--color-text-dim)] break-all leading-relaxed">
            <span className="text-[var(--color-text)]">PV:</span> {pv}
          </div>
        )}
      </div>
    </Panel>
  );
}

function WdlBar({ wdl }: { wdl: [number, number, number] }) {
  const total = wdl[0] + wdl[1] + wdl[2];
  if (total === 0) return null;
  const w = (wdl[0] / total) * 100;
  const d = (wdl[1] / total) * 100;

  return (
    <div className="h-2 rounded-full overflow-hidden flex bg-[var(--color-border)]">
      <div className="bg-white" style={{ width: `${w}%` }} />
      <div className="bg-gray-500" style={{ width: `${d}%` }} />
      <div className="bg-gray-900 flex-1" />
    </div>
  );
}
