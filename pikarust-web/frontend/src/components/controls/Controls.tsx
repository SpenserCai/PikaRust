import { Panel } from '@/components/ui/Panel';
import type { Phase, SearchSettings } from '@/lib/game';
import type { Side } from '@/lib/types';

interface Props {
  connected: boolean;
  phase: Phase;
  playerSide: Side;
  boardFlipped: boolean;
  canUndo: boolean;
  canRetry: boolean;
  settings: SearchSettings;
  onStartGame: () => void;
  onNewGame: () => void;
  onUndo: () => void;
  onRetry: () => void;
  onSetPlayerSide: (side: Side) => void;
  onToggleFlip: () => void;
  onSettingsChange: (settings: SearchSettings) => void;
}

export function Controls({ connected, phase, playerSide, boardFlipped, canUndo, canRetry, settings,
  onStartGame, onNewGame, onUndo, onRetry, onSetPlayerSide, onToggleFlip, onSettingsChange }: Props) {
  const btnBase = 'px-3 py-1.5 rounded text-xs font-bold transition disabled:opacity-40 disabled:cursor-not-allowed';
  const btnPrimary = `${btnBase} bg-[var(--color-accent)]/10 border border-[var(--color-accent)]/30 text-[var(--color-accent)] hover:bg-[var(--color-accent)]/20`;
  const btnSecondary = `${btnBase} bg-[var(--color-border)] border border-[var(--color-border)] text-[var(--color-text-dim)] hover:text-[var(--color-text)]`;
  const selectBase = 'px-2 py-1.5 rounded text-xs bg-[var(--color-bg)] border border-[var(--color-border)] text-[var(--color-text)] outline-none';

  return (
    <Panel title="Controls">
      <div className="flex flex-col gap-3">
        {phase === 'idle' && (
          <>
            <div className="flex items-center gap-2">
              <span className="text-xs text-[var(--color-text-dim)]">执棋：</span>
              <button onClick={() => onSetPlayerSide('w')} aria-pressed={playerSide === 'w'}
                className={`${btnBase} border ${playerSide === 'w' ? 'border-[var(--color-red-piece)] text-[var(--color-red-piece)] bg-[var(--color-red-piece)]/10' : 'border-[var(--color-border)] text-[var(--color-text-dim)]'}`}>
                红方
              </button>
              <button onClick={() => onSetPlayerSide('b')} aria-pressed={playerSide === 'b'}
                className={`${btnBase} border ${playerSide === 'b' ? 'border-[var(--color-black-piece)] text-[var(--color-black-piece)] bg-[var(--color-black-piece)]/10' : 'border-[var(--color-border)] text-[var(--color-text-dim)]'}`}>
                黑方
              </button>
            </div>
            <div className="flex items-center gap-2">
              <select aria-label="Search mode" value={settings.mode}
                onChange={event => onSettingsChange({ ...settings, mode: event.target.value as SearchSettings['mode'] })} className={selectBase}>
                <option value="depth">Depth</option>
                <option value="movetime">Time</option>
              </select>
              {settings.mode === 'depth' ? (
                <select aria-label="Search depth" value={settings.depth}
                  onChange={event => onSettingsChange({ ...settings, depth: Number(event.target.value) })} className={selectBase}>
                  {[6, 8, 10, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48].map(depth => <option key={depth} value={depth}>d={depth}</option>)}
                </select>
              ) : (
                <select aria-label="Move time" value={settings.movetime}
                  onChange={event => onSettingsChange({ ...settings, movetime: Number(event.target.value) })} className={selectBase}>
                  {[1000, 2000, 3000, 5000, 10000, 30000].map(ms => <option key={ms} value={ms}>{ms / 1000}s</option>)}
                </select>
              )}
            </div>
            <button onClick={onStartGame} disabled={!connected} className={btnPrimary}>开始对局</button>
          </>
        )}
        {phase !== 'idle' && (
          <div className="flex flex-wrap items-center gap-2">
            <button onClick={onNewGame} className={btnPrimary}>新对局</button>
            <button onClick={onUndo} disabled={!canUndo} className={btnSecondary}>悔棋</button>
            {canRetry && <button onClick={onRetry} disabled={!connected} className={btnSecondary}>重试</button>}
          </div>
        )}
        <button onClick={onToggleFlip} aria-pressed={boardFlipped} className={`${btnSecondary} ${boardFlipped ? 'text-[var(--color-accent)]' : ''}`}>
          🔄 翻转棋盘
        </button>
        <span className="flex items-center gap-1.5 text-xs text-[var(--color-text-dim)]">
          <span className={`inline-block w-2 h-2 rounded-full ${connected ? 'bg-green-400' : 'bg-red-500'}`} />
          {connected ? 'Connected' : 'Disconnected'}
        </span>
      </div>
    </Panel>
  );
}
