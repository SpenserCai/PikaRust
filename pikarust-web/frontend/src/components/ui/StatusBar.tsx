import type { GameStatus, Phase, TurnStatus } from '@/lib/game';
import type { Side } from '@/lib/types';

interface Props {
  currentSide: Side;
  playerSide: Side;
  phase: Phase;
  thinking: boolean;
  inCheck: boolean;
  gameStatus: GameStatus;
  status: TurnStatus;
  connected: boolean;
  error: string | null;
}

export function StatusBar({ currentSide, playerSide, phase, thinking, inCheck, gameStatus, status, connected, error }: Props) {
  let text: string;
  if (!connected) text = error ?? '正在连接引擎…';
  else if (error) text = error;
  else if (phase === 'idle') text = '等待开始';
  else if (gameStatus === 'draw') text = '对局结束，和棋';
  else if (gameStatus !== 'ongoing') {
    const humanWins = (gameStatus === 'win') === (currentSide === playerSide);
    text = humanWins ? '对局结束，你获胜' : '对局结束，AI 获胜';
  }
  else if (thinking) text = 'AI 思考中…';
  else if (status === 'syncing') text = '准备棋局…';
  else if (inCheck) text = '将军！轮到你走棋';
  else text = currentSide === playerSide ? '轮到你走棋' : (currentSide === 'w' ? '红方走棋' : '黑方走棋');

  return (
    <div role="status" aria-live="polite" data-testid="game-state" data-phase={phase} data-status={status}
      data-thinking={thinking} data-current-side={currentSide} data-player-side={playerSide} data-connected={connected} data-game-status={gameStatus}
      className="flex items-center justify-center gap-2 py-2 px-4 text-sm font-bold tracking-wide">
      {thinking && connected && <ThinkingDots />}
      <span className={`inline-block w-2.5 h-2.5 rounded-full ${currentSide === 'w' ? 'bg-[var(--color-red-piece)]' : 'bg-[var(--color-black-piece)]'}`} />
      <span className={thinking ? 'text-[var(--color-accent)] animate-pulse' : inCheck || error ? 'text-red-400' : 'text-[var(--color-text)]'}>{text}</span>
    </div>
  );
}

function ThinkingDots() {
  return (
    <span className="inline-flex gap-0.5" aria-hidden="true">
      {[0, 1, 2].map(index => <span key={index} className="w-1.5 h-1.5 rounded-full bg-[var(--color-accent)]"
        style={{ animation: `pulse-glow 1s ease-in-out ${index * 0.2}s infinite` }} />)}
    </span>
  );
}
