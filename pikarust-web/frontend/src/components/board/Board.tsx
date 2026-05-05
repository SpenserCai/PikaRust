import type { Move, Position, Square } from '@/lib/types';
import { BoardGrid } from './BoardGrid';
import { Piece } from './Piece';

interface BoardProps {
  position: Position;
  onSquareClick?: (square: Square) => void;
  selectedSquare?: Square | null;
  validMoves?: Square[];
  lastMove?: Move | null;
}

export function Board({ position, onSquareClick, selectedSquare, validMoves = [], lastMove }: BoardProps) {
  const padding = 0.6;
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

  const isLastMove = (r: number, c: number) =>
    lastMove && ((lastMove.from.row === r && lastMove.from.col === c) ||
      (lastMove.to.row === r && lastMove.to.col === c));

  return (
    <svg viewBox={`${-padding} ${-padding} ${w} ${h}`} onClick={handleClick}
      style={{ width: '100%', maxWidth: '480px', aspectRatio: `${w}/${h}` }}>
      <defs>
        <filter id="glow">
          <feGaussianBlur stdDeviation="0.06" result="blur" />
          <feMerge><feMergeNode in="blur" /><feMergeNode in="SourceGraphic" /></feMerge>
        </filter>
      </defs>

      <BoardGrid />

      {/* Valid move indicators */}
      {validMoves.map(({ row, col }) => (
        <circle key={`vm${row}-${col}`} cx={col} cy={row} r={0.15}
          fill="var(--color-accent)" opacity={0.5} />
      ))}

      {/* Pieces */}
      {position.map((row, r) =>
        row.map((piece, c) =>
          piece && (
            <Piece key={`${r}-${c}`} type={piece} x={c} y={r}
              selected={selectedSquare?.row === r && selectedSquare?.col === c}
              isLastMove={!!isLastMove(r, c)} />
          )
        )
      )}
    </svg>
  );
}
