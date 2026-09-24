export type PieceType = 'K' | 'A' | 'B' | 'N' | 'R' | 'C' | 'P' | 'k' | 'a' | 'b' | 'n' | 'r' | 'c' | 'p';
export type Position = (PieceType | null)[][];
export type Square = { row: number; col: number };
export type Move = { from: Square; to: Square };
export type Side = 'w' | 'b';
