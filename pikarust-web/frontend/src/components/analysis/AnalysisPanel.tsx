import type { AnalysisState } from '@/hooks/useAnalysis';
import type { Side } from '@/hooks/useGame';
import { Panel } from '@/components/ui/Panel';

interface Props {
  analysis: AnalysisState;
  playerSide: Side;
}

export function AnalysisPanel({ analysis, playerSide }: Props) {
  const { currentDepth, score, nodes, nps, pv, wdl } = analysis;

  // Engine outputs from AI's perspective. Normalize to red's perspective.
  const needsFlip = playerSide === 'w'; // AI is black, flip needed
  const redScore = needsFlip
    ? { cp: score.cp != null ? -score.cp : undefined, mate: score.mate != null ? -score.mate : undefined }
    : score;
  const redWdl: [number, number, number] | null = wdl
    ? (needsFlip ? [wdl[2], wdl[1], wdl[0]] : wdl)
    : null;

  const formattedScore = formatScore(redScore);

  return (
    <Panel title="Analysis">
      <div className="font-mono text-sm space-y-2">
        <div className="flex items-center gap-3">
          <span className="text-[var(--color-text-dim)]">d={currentDepth}</span>
          <span className="text-lg text-[var(--color-accent)] font-bold">{formattedScore}</span>
        </div>
        <div className="flex gap-3 text-xs text-[var(--color-text-dim)]">
          {nodes > 0 && <span>nodes: {formatNumber(nodes)}</span>}
          {nps > 0 && <span>nps: {formatNumber(nps)}</span>}
        </div>

        {redWdl && <WdlBar wdl={redWdl} />}

        {pv && (
          <div className="text-xs text-[var(--color-text-dim)] break-all leading-relaxed pt-1 border-t border-[var(--color-border)]">
            <span className="text-[var(--color-text)] font-bold">PV</span>{' '}
            <span className="opacity-80">{pv}</span>
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
    <div className="space-y-1">
      <div className="h-2 rounded-full overflow-hidden flex bg-[var(--color-border)]">
        <div className="bg-[var(--color-red-piece)] transition-all duration-300" style={{ width: `${w}%` }} />
        <div className="bg-gray-500 transition-all duration-300" style={{ width: `${d}%` }} />
        <div className="bg-[var(--color-black-piece)] flex-1 transition-all duration-300" />
      </div>
      <div className="flex justify-between text-[10px] text-[var(--color-text-dim)]">
        <span>红 {wdl[0]}‰</span>
        <span>和 {wdl[1]}‰</span>
        <span>黑 {wdl[2]}‰</span>
      </div>
    </div>
  );
}

function formatScore(score: { cp?: number; mate?: number }): string {
  if (score.mate != null) return `M${score.mate}`;
  if (score.cp != null) return (score.cp >= 0 ? '+' : '') + (score.cp / 100).toFixed(2);
  return '0.00';
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(0)}k`;
  return String(n);
}
