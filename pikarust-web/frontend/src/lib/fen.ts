import type { PieceType, Position } from './types';

export const INITIAL_FEN = 'rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1';

const VALID_PIECES = 'KABNRCPkabnrcp';

export function parseFen(fen: string): Position {
  const board = fen.split(' ')[0] ?? '';
  const rows = board.split('/');
  const position: Position = [];
  for (const row of rows) {
    const rank: (PieceType | null)[] = [];
    for (const ch of row) {
      if (ch >= '1' && ch <= '9') {
        for (let i = 0; i < parseInt(ch); i++) rank.push(null);
      } else if (VALID_PIECES.includes(ch)) {
        rank.push(ch as PieceType);
      }
    }
    position.push(rank);
  }
  return position;
}

export function positionToFen(position: Position): string {
  return position.map(row => {
    let fen = '';
    let empty = 0;
    for (const cell of row) {
      if (cell === null) { empty++; }
      else {
        if (empty > 0) { fen += empty; empty = 0; }
        fen += cell;
      }
    }
    if (empty > 0) fen += empty;
    return fen;
  }).join('/');
}
