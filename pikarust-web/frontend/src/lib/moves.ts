import type { Position, Square } from './types.ts';
import { squareToUci, uciToSquare } from './game.ts';

/** The engine supplies the legal set; the browser only maps board coordinates. */
export function legalTargets(legalMoves: string[], from: Square): Square[] {
  const prefix = squareToUci(from);
  return legalMoves.filter(move => move.startsWith(prefix))
    .map(move => uciToSquare(move.slice(2)))
    .filter((square): square is Square => square !== null);
}

export function applyMoveToPosition(position: Position, move: string): void {
  if (!/^[a-i][0-9][a-i][0-9]$/.test(move)) return;
  const from = uciToSquare(move.slice(0, 2));
  const to = uciToSquare(move.slice(2));
  if (!from || !to) return;
  const fromRow = position[from.row];
  const toRow = position[to.row];
  if (!fromRow || !toRow) return;
  toRow[to.col] = fromRow[from.col] ?? null;
  fromRow[from.col] = null;
}

export function findKing(position: Position, king: 'K' | 'k'): Square | null {
  for (let row = 0; row < position.length; row++) {
    const col = position[row]?.indexOf(king) ?? -1;
    if (col !== -1) return { row, col };
  }
  return null;
}
