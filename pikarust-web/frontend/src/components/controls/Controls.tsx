import { useState } from 'react';
import { Panel } from '@/components/ui/Panel';
import type { Phase, Side } from '@/hooks/useGame';

interface Props {
  connected: boolean;
  phase: Phase;
  playerSide: Side;
  onStartGame: () => void;
  onNewGame: () => void;
  onUndo: () => void;
  onSetPlayerSide: (side: Side) => void;
  onSetDepth: (depth: number) => void;
  onSetMovetime: (ms: number) => void;
}

export function Controls({ connected, phase, playerSide, onStartGame, onNewGame, onUndo, onSetPlayerSide, onSetDepth, onSetMovetime }: Props) {
  const [depth, setDepth] = useState(12);
  const [movetime, setMovetime] = useState(3000);
  const [mode, setMode] = useState<'depth' | 'movetime'>('depth');

  const btnBase = 'px-3 py-1.5 rounded text-xs font-bold transition';
  const btnPrimary = `${btnBase} bg-[var(--color-accent)]/10 border border-[var(--color-accent)]/30 text-[var(--color-accent)] hover:bg-[var(--color-accent)]/20`;
  const btnSecondary = `${btnBase} bg-[var(--color-border)] border border-[var(--color-border)] text-[var(--color-text-dim)] hover:text-[var(--color-text)]`;
  const selectBase = 'px-2 py-1.5 rounded text-xs bg-[var(--color-bg)] border border-[var(--color-border)] text-[var(--color-text)] outline-none';

  return (
    <Panel title="Controls">
      <div className="flex flex-col gap-3">
        {phase === 'idle' && (
          <>
            {/* Side selection */}
            <div className="flex items-center gap-2">
              <span className="text-xs text-[var(--color-text-dim)]">执棋：</span>
              <button onClick={() => onSetPlayerSide('w')}
                className={`${btnBase} border ${playerSide === 'w' ? 'border-[var(--color-red-piece)] text-[var(--color-red-piece)] bg-[var(--color-red-piece)]/10' : 'border-[var(--color-border)] text-[var(--color-text-dim)]'}`}>
                红方
              </button>
              <button onClick={() => onSetPlayerSide('b')}
                className={`${btnBase} border ${playerSide === 'b' ? 'border-[var(--color-black-piece)] text-[var(--color-black-piece)] bg-[var(--color-black-piece)]/10' : 'border-[var(--color-border)] text-[var(--color-text-dim)]'}`}>
                黑方
              </button>
            </div>
            {/* Engine settings */}
            <div className="flex items-center gap-2">
              <select value={mode} onChange={(e) => setMode(e.target.value as 'depth' | 'movetime')} className={selectBase}>
                <option value="depth">Depth</option>
                <option value="movetime">Time</option>
              </select>
              {mode === 'depth' ? (
                <select value={depth} onChange={(e) => { const d = Number(e.target.value); setDepth(d); onSetDepth(d); onSetMovetime(0); }} className={selectBase}>
                  {[6, 8, 10, 12, 16, 20, 24].map((d) => <option key={d} value={d}>d={d}</option>)}
                </select>
              ) : (
                <select value={movetime} onChange={(e) => { const ms = Number(e.target.value); setMovetime(ms); onSetMovetime(ms); onSetDepth(0); }} className={selectBase}>
                  {[1000, 2000, 3000, 5000, 10000, 30000].map((ms) => <option key={ms} value={ms}>{ms / 1000}s</option>)}
                </select>
              )}
            </div>
            {/* Start button */}
            <button onClick={onStartGame} disabled={!connected} className={btnPrimary}>
              开始对局
            </button>
          </>
        )}

        {(phase === 'playing' || phase === 'ended') && (
          <div className="flex flex-wrap items-center gap-2">
            <button onClick={onNewGame} className={btnPrimary}>新对局</button>
            {phase === 'playing' && <button onClick={onUndo} className={btnSecondary}>悔棋</button>}
          </div>
        )}

        {/* Connection status */}
        <span className="flex items-center gap-1.5 text-xs text-[var(--color-text-dim)]">
          <span className={`inline-block w-2 h-2 rounded-full ${connected ? 'bg-green-400' : 'bg-red-500'}`} />
          {connected ? 'Connected' : 'Disconnected'}
        </span>
      </div>
    </Panel>
  );
}
