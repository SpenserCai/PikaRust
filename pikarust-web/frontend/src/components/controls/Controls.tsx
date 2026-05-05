import { useState } from 'react';
import { Panel } from '@/components/ui/Panel';

interface Props {
  connected: boolean;
  onNewGame: () => void;
  onUndo: () => void;
  onSetDepth: (depth: number) => void;
}

export function Controls({ connected, onNewGame, onUndo, onSetDepth }: Props) {
  const [depth, setDepth] = useState(12);

  return (
    <Panel title="Controls">
      <div className="flex flex-wrap items-center gap-2">
        <button onClick={onNewGame} className="px-3 py-1.5 rounded text-xs font-bold bg-[var(--color-accent)]/10 border border-[var(--color-accent)]/30 text-[var(--color-accent)] hover:bg-[var(--color-accent)]/20 transition">
          New Game
        </button>
        <button onClick={onUndo} className="px-3 py-1.5 rounded text-xs font-bold bg-[var(--color-border)] border border-[var(--color-border)] text-[var(--color-text-dim)] hover:text-[var(--color-text)] transition">
          Undo
        </button>
        <select
          value={depth}
          onChange={(e) => { const d = Number(e.target.value); setDepth(d); onSetDepth(d); }}
          className="px-2 py-1.5 rounded text-xs bg-[var(--color-bg)] border border-[var(--color-border)] text-[var(--color-text)] outline-none"
        >
          {[6, 8, 10, 12, 16, 20, 24].map((d) => <option key={d} value={d}>Depth {d}</option>)}
        </select>
        <span className="ml-auto flex items-center gap-1.5 text-xs text-[var(--color-text-dim)]">
          <span className={`inline-block w-2 h-2 rounded-full ${connected ? 'bg-green-400' : 'bg-red-500'}`} />
          {connected ? 'Connected' : 'Disconnected'}
        </span>
      </div>
    </Panel>
  );
}
