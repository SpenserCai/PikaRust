import type { Phase, Side } from '@/hooks/useGame';

interface Props {
  currentSide: Side;
  playerSide: Side;
  phase: Phase;
  thinking: boolean;
  inCheck: boolean;
  gameOver: boolean;
}

export function StatusBar({ currentSide, playerSide, phase, thinking, inCheck, gameOver }: Props) {
  let text: string;
  let accent = false;

  if (phase === 'idle') {
    text = '等待开始';
  } else if (gameOver) {
    text = '游戏结束';
  } else if (thinking) {
    text = 'AI 思考中...';
    accent = true;
  } else if (inCheck) {
    text = '将军！';
  } else {
    const isPlayerTurn = currentSide === playerSide;
    text = isPlayerTurn ? '轮到你走棋' : (currentSide === 'w' ? '红方走棋' : '黑方走棋');
  }

  return (
    <div className="flex items-center justify-center gap-2 py-2 px-4 text-sm font-bold tracking-wide">
      {thinking && <ThinkingDots />}
      <span className={`inline-block w-2.5 h-2.5 rounded-full ${currentSide === 'w' ? 'bg-[var(--color-red-piece)]' : 'bg-[var(--color-black-piece)]'}`} />
      <span className={accent ? 'text-[var(--color-accent)] animate-pulse' : inCheck ? 'text-red-400' : 'text-[var(--color-text)]'}>
        {text}
      </span>
    </div>
  );
}

function ThinkingDots() {
  return (
    <span className="inline-flex gap-0.5">
      {[0, 1, 2].map(i => (
        <span key={i} className="w-1.5 h-1.5 rounded-full bg-[var(--color-accent)]"
          style={{ animation: `pulse-glow 1s ease-in-out ${i * 0.2}s infinite` }} />
      ))}
    </span>
  );
}
