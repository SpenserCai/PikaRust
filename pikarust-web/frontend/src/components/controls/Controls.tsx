import { useState } from 'react';
import { Panel } from '@/components/ui/Panel';

interface Props {
  connected: boolean;
  onNewGame: () => void;
  onUndo: () => void;
  onSetDepth: (depth: number) => void;
  onSetMovetime: (ms: number) => void;
}

export function Controls({ connected, onNewGame, onUndo, onSetDepth, onSetMovetime }: Props) {
  const [depth, setDepth] = useState(12);
  const [movetime, setMovetime] = useState(3000);
  const [mode, setMode] = useState<'depth' | 'movetime'>('depth');

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
          value={mode}
          onChange={(e) => setMode(e.target.value as 'depth' | 'movetime')}
          className="px-2 py-1.5 rounded text-xs bg-[var(--color-bg)] border border-[var(--color-border)] text-[var(--color-text)] outline-none"
        >
          <option value="depth">Depth</option>
          <option value="movetime">Time</option>
        </select>
        {mode === 'depth' ? (
          <select
            value={depth}
            onChange={(e) => { const d = Number(e.target.value); setDepth(d); onSetDepth(d); onSetMovetime(0); }}
            className="px-2 py-1.5 rounded text-xs bg-[var(--color-bg)] border border-[var(--color-border)] text-[var(--color-text)] outline-none"
          >
            {[6, 8, 10, 12, 16, 20, 24].map((d) => <option key={d} value={d}>d={d}</option>)}
          </select>
        ) : (
          <select
            value={movetime}
            onChange={(e) => { const ms = Number(e.target.value); setMovetime(ms); onSetMovetime(ms); onSetDepth(0); }}
            className="px-2 py-1.5 rounded text-xs bg-[var(--color-bg)] border border-[var(--color-border)] text-[var(--color-text)] outline-none"
          >
            {[1000, 2000, 3000, 5000, 10000, 30000].map((ms) => <option key={ms} value={ms}>{ms / 1000}s</option>)}
          </select>
        )}
        <span className="ml-auto flex items-center gap-1.5 text-xs text-[var(--color-text-dim)]">
          <span className={`inline-block w-2 h-2 rounded-full ${connected ? 'bg-green-400' : 'bg-red-500'}`} />
          {connected ? 'Connected' : 'Disconnected'}
        </span>
      </div>
    </Panel>
  );
}
