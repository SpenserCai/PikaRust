import { useEffect, useCallback, useState, useMemo } from 'react';
import { useEngine } from '@/hooks/useEngine';
import { useGame } from '@/hooks/useGame';
import { useAnalysis } from '@/hooks/useAnalysis';
import { Layout } from '@/components/layout/Layout';
import { Board } from '@/components/board';
import { AnalysisPanel } from '@/components/analysis/AnalysisPanel';
import { Controls } from '@/components/controls/Controls';
import { MoveHistory } from '@/components/history/MoveHistory';
import { StatusBar } from '@/components/ui/StatusBar';
import { parseFen } from '@/lib/fen';
import { getValidMoves } from '@/lib/moves';
import type { Position, Square } from '@/lib/types';

export default function App() {
  const { connected, sendCommand, onMessage, bestMove } = useEngine();
  const [depth, setDepth] = useState(12);
  const [movetime, setMovetime] = useState(0);
  const game = useGame(sendCommand, depth, movetime);
  const analysis = useAnalysis(onMessage);
  const [selectedSquare, setSelectedSquare] = useState<Square | null>(null);
  const [validMoves, setValidMoves] = useState<Square[]>([]);

  const boardPosition = useMemo(() => {
    const pos = parseFen(game.fen);
    for (const move of game.moveHistory) {
      applyMoveToPosition(pos, move);
    }
    return pos;
  }, [game.fen, game.moveHistory]);

  const inCheck = useMemo(() => {
    if (game.currentSide !== game.playerSide) return false;
    return isKingInCheck(boardPosition, game.playerSide);
  }, [boardPosition, game.currentSide, game.playerSide]);

  useEffect(() => {
    if (game.moveHistory.length === 0) {
      setSelectedSquare(null);
      setValidMoves([]);
    }
  }, [game.moveHistory.length]);

  useEffect(() => {
    if (bestMove && !game.gameOver && game.phase === 'playing') {
      game.applyEngineMove(bestMove.move);
    }
  }, [bestMove]); // eslint-disable-line react-hooks/exhaustive-deps

  const isPlayerPiece = useCallback((piece: string) => {
    return game.playerSide === 'w'
      ? (piece >= 'A' && piece <= 'Z')
      : (piece >= 'a' && piece <= 'z');
  }, [game.playerSide]);

  const handleSquareClick = useCallback((square: Square) => {
    if (game.phase !== 'playing' || game.currentSide !== game.playerSide || game.gameOver) return;

    const piece = boardPosition[square.row]?.[square.col] ?? null;

    if (selectedSquare && validMoves.some(m => m.row === square.row && m.col === square.col)) {
      game.makeMove([selectedSquare.col, selectedSquare.row], [square.col, square.row]);
      setSelectedSquare(null);
      setValidMoves([]);
      return;
    }

    if (piece && isPlayerPiece(piece)) {
      setSelectedSquare(square);
      setValidMoves(getValidMoves(boardPosition, square));
    } else {
      setSelectedSquare(null);
      setValidMoves([]);
    }
  }, [selectedSquare, validMoves, boardPosition, game, isPlayerPiece]);

  const handleSetDepth = (d: number) => setDepth(d);
  const handleSetMovetime = (ms: number) => setMovetime(ms);

  const lastMove = game.moveHistory.length > 0
    ? (() => {
        const m = game.moveHistory[game.moveHistory.length - 1]!;
        const from = uciToSquare(m.slice(0, 2));
        const to = uciToSquare(m.slice(2, 4));
        return from && to ? { from, to } : null;
      })()
    : null;

  const board = (
    <Board
      position={boardPosition}
      onSquareClick={handleSquareClick}
      selectedSquare={selectedSquare}
      validMoves={validMoves}
      lastMove={lastMove}
      inCheck={inCheck}
    />
  );

  const statusBar = (
    <StatusBar
      currentSide={game.currentSide}
      playerSide={game.playerSide}
      phase={game.phase}
      thinking={game.thinking}
      inCheck={inCheck}
      gameOver={game.gameOver}
    />
  );

  const sidePanel = (
    <>
      <Controls
        connected={connected}
        phase={game.phase}
        playerSide={game.playerSide}
        onStartGame={game.startGame}
        onNewGame={game.newGame}
        onUndo={game.undo}
        onSetPlayerSide={game.setPlayerSide}
        onSetDepth={handleSetDepth}
        onSetMovetime={handleSetMovetime}
      />
      <AnalysisPanel analysis={analysis} />
      <MoveHistory moves={game.moveHistory} />
    </>
  );

  return <Layout board={board} sidePanel={sidePanel} statusBar={statusBar} />;
}

function uciToSquare(s: string): Square | null {
  if (s.length < 2) return null;
  const col = s.charCodeAt(0) - 97;
  const rank = parseInt(s[1]!);
  if (col < 0 || col > 8 || isNaN(rank) || rank < 0 || rank > 9) return null;
  return { row: 9 - rank, col };
}

function applyMoveToPosition(pos: Position, uciMove: string): void {
  const from = uciToSquare(uciMove.slice(0, 2));
  const to = uciToSquare(uciMove.slice(2, 4));
  if (!from || !to) return;
  const fromRow = pos[from.row];
  const toRow = pos[to.row];
  if (fromRow && toRow) {
    toRow[to.col] = fromRow[from.col] ?? null;
    fromRow[from.col] = null;
  }
}

function isKingInCheck(pos: Position, side: 'w' | 'b'): boolean {
  const king = side === 'w' ? 'K' : 'k';
  let kr = -1, kc = -1;
  for (let r = 0; r < 10; r++) {
    for (let c = 0; c < 9; c++) {
      if (pos[r]?.[c] === king) { kr = r; kc = c; }
    }
  }
  if (kr < 0) return false;

  for (let r = 0; r < 10; r++) {
    for (let c = 0; c < 9; c++) {
      const p = pos[r]?.[c];
      if (!p) continue;
      const isOpponent = side === 'w' ? (p >= 'a' && p <= 'z') : (p >= 'A' && p <= 'Z');
      if (!isOpponent) continue;
      if (canAttack(pos, p, r, c, kr, kc)) return true;
    }
  }
  return false;
}

function canAttack(pos: Position, piece: string, fr: number, fc: number, tr: number, tc: number): boolean {
  const type = piece.toUpperCase();
  const dr = tr - fr, dc = tc - fc;

  if (type === 'R') {
    if (dr !== 0 && dc !== 0) return false;
    return pathClear(pos, fr, fc, tr, tc, 0);
  }
  if (type === 'C') {
    if (dr !== 0 && dc !== 0) return false;
    return pathClear(pos, fr, fc, tr, tc, 1);
  }
  if (type === 'N') {
    const adx = Math.abs(dc), ady = Math.abs(dr);
    if (!((adx === 1 && ady === 2) || (adx === 2 && ady === 1))) return false;
    if (ady === 2) { return pos[fr + (dr > 0 ? 1 : -1)]?.[fc] == null; }
    return pos[fr]?.[fc + (dc > 0 ? 1 : -1)] == null;
  }
  if (type === 'P') {
    const red = piece >= 'A' && piece <= 'Z';
    const forward = red ? -1 : 1;
    if (dr === forward && dc === 0) return true;
    const crossed = red ? fr <= 4 : fr >= 5;
    if (crossed && dr === 0 && Math.abs(dc) === 1) return true;
    return false;
  }
  if (type === 'K') {
    if (dc === 0) return pathClear(pos, fr, fc, tr, tc, 0);
    return false;
  }
  return false;
}

function pathClear(pos: Position, fr: number, fc: number, tr: number, tc: number, requiredBetween: number): boolean {
  let count = 0;
  if (fr === tr) {
    const step = tc > fc ? 1 : -1;
    for (let c = fc + step; c !== tc; c += step) {
      if (pos[fr]?.[c] != null) count++;
    }
  } else {
    const step = tr > fr ? 1 : -1;
    for (let r = fr + step; r !== tr; r += step) {
      if (pos[r]?.[fc] != null) count++;
    }
  }
  return count === requiredBetween;
}
