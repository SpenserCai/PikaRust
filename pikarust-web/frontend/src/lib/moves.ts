import type { PieceType, Position, Square } from './types';

function isRed(p: PieceType): boolean { return p >= 'A' && p <= 'Z'; }
function sameColor(a: PieceType, b: PieceType): boolean { return isRed(a) === isRed(b); }
function inBoard(r: number, c: number): boolean { return r >= 0 && r <= 9 && c >= 0 && c <= 8; }

function inPalace(r: number, c: number, red: boolean): boolean {
  const colOk = c >= 3 && c <= 5;
  return colOk && (red ? (r >= 7 && r <= 9) : (r >= 0 && r <= 2));
}

function getCell(pos: Position, r: number, c: number): PieceType | null {
  return pos[r]?.[c] ?? null;
}

function kingsNotFacing(pos: Position, from: Square, to: Square, piece: PieceType): boolean {
  const captured = getCell(pos, to.row, to.col);
  pos[from.row]![from.col] = null;
  pos[to.row]![to.col] = piece;

  let rk: Square | null = null, bk: Square | null = null;
  for (let r = 0; r < 10; r++) {
    for (let c = 3; c <= 5; c++) {
      const cell = getCell(pos, r, c);
      if (cell === 'K') rk = { row: r, col: c };
      if (cell === 'k') bk = { row: r, col: c };
    }
  }

  pos[from.row]![from.col] = piece;
  pos[to.row]![to.col] = captured;

  if (!rk || !bk || rk.col !== bk.col) return true;
  for (let r = bk.row + 1; r < rk.row; r++) {
    if (getCell(pos, r, rk.col) !== null) return true;
  }
  return false;
}

const ORTHO: [number, number][] = [[0, 1], [0, -1], [1, 0], [-1, 0]];
const DIAG: [number, number][] = [[1, 1], [1, -1], [-1, 1], [-1, -1]];
const BISHOP_DIRS: [number, number][] = [[2, 2], [2, -2], [-2, 2], [-2, -2]];
const KNIGHT_LEGS: [number, number, number, number][] = [
  [-1, 0, -2, -1], [-1, 0, -2, 1],
  [1, 0, 2, -1], [1, 0, 2, 1],
  [0, -1, -1, -2], [0, -1, 1, -2],
  [0, 1, -1, 2], [0, 1, 1, 2],
];

function getRawMoves(pos: Position, sq: Square): Square[] {
  const piece = getCell(pos, sq.row, sq.col);
  if (!piece) return [];
  const red = isRed(piece);
  const moves: Square[] = [];
  const { row: r, col: c } = sq;
  const type = piece.toUpperCase();

  if (type === 'K') {
    for (const [dr, dc] of ORTHO) {
      const nr = r + dr, nc = c + dc;
      if (inPalace(nr, nc, red)) moves.push({ row: nr, col: nc });
    }
  } else if (type === 'A') {
    for (const [dr, dc] of DIAG) {
      const nr = r + dr, nc = c + dc;
      if (inPalace(nr, nc, red)) moves.push({ row: nr, col: nc });
    }
  } else if (type === 'B') {
    for (const [dr, dc] of BISHOP_DIRS) {
      const nr = r + dr, nc = c + dc;
      if (!inBoard(nr, nc)) continue;
      if (red && nr < 5) continue;
      if (!red && nr > 4) continue;
      if (getCell(pos, r + dr / 2, c + dc / 2) !== null) continue;
      moves.push({ row: nr, col: nc });
    }
  } else if (type === 'N') {
    for (const [lr, lc, dr, dc] of KNIGHT_LEGS) {
      if (getCell(pos, r + lr, c + lc) !== null) continue;
      const nr = r + dr, nc = c + dc;
      if (inBoard(nr, nc)) moves.push({ row: nr, col: nc });
    }
  } else if (type === 'R') {
    for (const [dr, dc] of ORTHO) {
      for (let i = 1; ; i++) {
        const nr = r + dr * i, nc = c + dc * i;
        if (!inBoard(nr, nc)) break;
        moves.push({ row: nr, col: nc });
        if (getCell(pos, nr, nc) !== null) break;
      }
    }
  } else if (type === 'C') {
    for (const [dr, dc] of ORTHO) {
      let jumped = false;
      for (let i = 1; ; i++) {
        const nr = r + dr * i, nc = c + dc * i;
        if (!inBoard(nr, nc)) break;
        if (!jumped) {
          if (getCell(pos, nr, nc) === null) moves.push({ row: nr, col: nc });
          else jumped = true;
        } else {
          if (getCell(pos, nr, nc) !== null) { moves.push({ row: nr, col: nc }); break; }
        }
      }
    }
  } else if (type === 'P') {
    const forward = red ? -1 : 1;
    const crossedRiver = red ? r <= 4 : r >= 5;
    const dirs: [number, number][] = [[forward, 0]];
    if (crossedRiver) { dirs.push([0, -1], [0, 1]); }
    for (const [dr, dc] of dirs) {
      const nr = r + dr, nc = c + dc;
      if (inBoard(nr, nc)) moves.push({ row: nr, col: nc });
    }
  }

  return moves;
}

export function getValidMoves(position: Position, square: Square): Square[] {
  const piece = getCell(position, square.row, square.col);
  if (!piece) return [];
  return getRawMoves(position, square).filter(to => {
    const target = getCell(position, to.row, to.col);
    if (target && sameColor(piece, target)) return false;
    return kingsNotFacing(position, square, to, piece);
  });
}
