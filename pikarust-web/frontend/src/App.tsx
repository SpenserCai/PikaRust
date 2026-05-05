import { useEffect, useCallback, useState, useMemo } from 'react';
import { useEngine } from '@/hooks/useEngine';
import { useGame } from '@/hooks/useGame';
import { useAnalysis } from '@/hooks/useAnalysis';
import { Layout } from '@/components/layout/Layout';
import { Board } from '@/components/board';
import { AnalysisPanel } from '@/components/analysis/AnalysisPanel';
import { Controls } from '@/components/controls/Controls';
import { MoveHistory } from '@/components/history/MoveHistory';
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

  // Derive board position from FEN + move history (single source of truth)
  const boardPosition = useMemo(() => {
    const pos = parseFen(game.fen);
    for (const move of game.moveHistory) {
      applyMoveToPosition(pos, move);
    }
    return pos;
  }, [game.fen, game.moveHistory]);

  // Reset selection on new game
  useEffect(() => {
    if (game.moveHistory.length === 0) {
      setSelectedSquare(null);
      setValidMoves([]);
    }
  }, [game.moveHistory.length]);

  // Apply engine's best move
  useEffect(() => {
    if (bestMove && !game.gameOver) {
      // Only apply if it's the engine's turn (black)
      // Since makeMove already flipped to 'b' and sent go,
      // when bestMove arrives currentSide should still be 'b'
      game.applyEngineMove(bestMove.move);
    }
  }, [bestMove]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleSquareClick = useCallback((square: Square) => {
    if (game.currentSide !== 'w' || game.gameOver) return;

    const piece = boardPosition[square.row]?.[square.col] ?? null;

    // If a piece is selected and clicking a valid move target
    if (selectedSquare && validMoves.some(m => m.row === square.row && m.col === square.col)) {
      game.makeMove([selectedSquare.col, selectedSquare.row], [square.col, square.row]);
      setSelectedSquare(null);
      setValidMoves([]);
      return;
    }

    // Select a red piece
    if (piece && piece >= 'A' && piece <= 'Z') {
      setSelectedSquare(square);
      setValidMoves(getValidMoves(boardPosition, square));
    } else {
      setSelectedSquare(null);
      setValidMoves([]);
    }
  }, [selectedSquare, validMoves, boardPosition, game]);

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
    />
  );

  const sidePanel = (
    <>
      <Controls connected={connected} onNewGame={game.newGame} onUndo={game.undo} onSetDepth={handleSetDepth} onSetMovetime={handleSetMovetime} />
      <AnalysisPanel analysis={analysis} />
      <MoveHistory moves={game.moveHistory} />
    </>
  );

  return <Layout board={board} sidePanel={sidePanel} />;
}

function uciToSquare(s: string): Square | null {
  if (s.length < 2) return null;
  const col = s.charCodeAt(0) - 97; // a=0, i=8
  const rank = parseInt(s[1]!);
  if (col < 0 || col > 8 || isNaN(rank) || rank < 0 || rank > 9) return null;
  return { row: 9 - rank, col }; // rank 0 = array row 9, rank 9 = array row 0
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
