import { Panel } from '@/components/ui/Panel';

interface Props {
  moves: string[];
}

export function MoveHistory({ moves }: Props) {
  const pairs: [string, string | undefined][] = [];
  for (let i = 0; i < moves.length; i += 2) {
    pairs.push([moves[i]!, moves[i + 1]]);
  }

  return (
    <Panel title="Moves" className="flex-1 min-h-0">
      <div className="max-h-48 overflow-y-auto font-mono text-xs space-y-0.5">
        {pairs.length === 0 && <span className="text-[var(--color-text-dim)]">No moves yet</span>}
        {pairs.map(([red, black], i) => {
          const redIdx = i * 2;
          const blackIdx = i * 2 + 1;
          const isCurrent = moves.length - 1 === redIdx || moves.length - 1 === blackIdx;
          return (
            <div key={i} className={`flex gap-2 px-1 rounded ${isCurrent ? 'bg-[var(--color-accent)]/10' : ''}`}>
              <span className="text-[var(--color-text-dim)] w-5">{i + 1}.</span>
              <span className={`w-12 ${moves.length - 1 === redIdx ? 'text-[var(--color-red-piece)]' : 'text-[var(--color-text)]'}`}>{red}</span>
              {black && <span className={`w-12 ${moves.length - 1 === blackIdx ? 'text-[var(--color-black-piece)]' : 'text-[var(--color-text)]'}`}>{black}</span>}
            </div>
          );
        })}
      </div>
    </Panel>
  );
}
