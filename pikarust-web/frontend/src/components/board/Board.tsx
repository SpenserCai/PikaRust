import type { Move, Position, Square } from '@/lib/types';
import { BoardGrid } from './BoardGrid';
import { Piece } from './Piece';

interface BoardProps {
  position: Position;
  onSquareClick?: (square: Square) => void;
  selectedSquare?: Square | null;
  validMoves?: Square[];
  lastMove?: Move | null;
  inCheck?: boolean;
}

export function Board({ position, onSquareClick, selectedSquare, validMoves = [], lastMove, inCheck }: BoardProps) {
  const padding = 0.8;
  const w = 8 + padding * 2;
  const h = 9 + padding * 2;

  function handleClick(e: React.MouseEvent<SVGSVGElement>) {
    if (!onSquareClick) return;
    const svg = e.currentTarget;
    const rect = svg.getBoundingClientRect();
    const x = ((e.clientX - rect.left) / rect.width) * w - padding;
    const y = ((e.clientY - rect.top) / rect.height) * h - padding;
    const col = Math.round(x);
    const row = Math.round(y);
    if (col >= 0 && col <= 8 && row >= 0 && row <= 9) {
      onSquareClick({ row, col });
    }
  }

  // Find king position for check highlight
  const kingPos = inCheck ? findKing(position, 'K') : null;

  return (
    <svg
      viewBox={`${-padding} ${-padding} ${w} ${h}`}
      onClick={handleClick}
      className="w-full max-w-[600px] lg:max-w-[640px]"
      style={{ aspectRatio: `${w}/${h}` }}
    >
      <defs>
        <filter id="glow">
          <feGaussianBlur stdDeviation="0.06" result="blur" />
          <feMerge><feMergeNode in="blur" /><feMergeNode in="SourceGraphic" /></feMerge>
        </filter>
        <filter id="glow-check">
          <feGaussianBlur stdDeviation="0.12" result="blur" />
          <feMerge><feMergeNode in="blur" /><feMergeNode in="SourceGraphic" /></feMerge>
        </filter>
      </defs>

      {/* Board background with rounded corners */}
      <rect x={-padding + 0.1} y={-padding + 0.1} width={w - 0.2} height={h - 0.2}
        rx={0.3} ry={0.3} fill="var(--color-surface)" stroke="var(--color-border)" strokeWidth={0.04} />

      <BoardGrid />

      {/* Last move highlights */}
      {lastMove && (
        <>
          <rect x={lastMove.from.col - 0.4} y={lastMove.from.row - 0.4} width={0.8} height={0.8}
            rx={0.08} fill="var(--color-accent2)" opacity={0.12} />
          <rect x={lastMove.to.col - 0.4} y={lastMove.to.row - 0.4} width={0.8} height={0.8}
            rx={0.08} fill="var(--color-accent2)" opacity={0.25} />
        </>
      )}

      {/* Check highlight on king */}
      {kingPos && (
        <circle cx={kingPos.col} cy={kingPos.row} r={0.5}
          fill="rgba(255,80,80,0.25)" stroke="rgba(255,80,80,0.6)" strokeWidth={0.06}
          filter="url(#glow-check)">
          <animate attributeName="opacity" values="0.6;1;0.6" dur="1s" repeatCount="indefinite" />
        </circle>
      )}

      {/* Valid move indicators */}
      {validMoves.map(({ row, col }) => {
        const isCapture = position[row]?.[col] != null;
        return isCapture ? (
          <circle key={`vm${row}-${col}`} cx={col} cy={row} r={0.42}
            fill="none" stroke="var(--color-accent)" strokeWidth={0.06} opacity={0.6} />
        ) : (
          <circle key={`vm${row}-${col}`} cx={col} cy={row} r={0.12}
            fill="var(--color-accent)" opacity={0.5} />
        );
      })}

      {/* Pieces */}
      {position.map((row, r) =>
        row.map((piece, c) =>
          piece && (
            <Piece key={`${r}-${c}`} type={piece} x={c} y={r}
              selected={selectedSquare?.row === r && selectedSquare?.col === c}
              isLastMove={!!(lastMove && lastMove.to.row === r && lastMove.to.col === c)} />
          )
        )
      )}
    </svg>
  );
}

function findKing(pos: Position, king: 'K' | 'k'): Square | null {
  for (let r = 0; r < 10; r++) {
    for (let c = 0; c < 9; c++) {
      if (pos[r]?.[c] === king) return { row: r, col: c };
    }
  }
  return null;
}
