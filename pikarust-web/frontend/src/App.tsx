import { useEffect, useCallback, useState } from 'react';
import { useEngine } from '@/hooks/useEngine';
import { useGame } from '@/hooks/useGame';
import { useAnalysis } from '@/hooks/useAnalysis';
import { Layout } from '@/components/layout/Layout';
import { Board } from '@/components/board';
import { AnalysisPanel } from '@/components/analysis/AnalysisPanel';
import { Controls } from '@/components/controls/Controls';
import { MoveHistory } from '@/components/history/MoveHistory';
import { parseFen, INITIAL_FEN } from '@/lib/fen';
import { getValidMoves } from '@/lib/moves';
import type { Square } from '@/lib/types';

export default function App() {
  const { connected, sendCommand, onMessage, bestMove } = useEngine();
  const [depth, setDepth] = useState(12);
  const [movetime, setMovetime] = useState(0);
  const game = useGame(sendCommand, depth, movetime);
  const analysis = useAnalysis(onMessage);
  const [selectedSquare, setSelectedSquare] = useState<Square | null>(null);
  const [validMoves, setValidMoves] = useState<Square[]>([]);

  const position = parseFen(
    game.moveHistory.length === 0 ? game.fen : game.fen
  );

  // Recompute position from FEN + moves is complex; for now use FEN directly.
  // The real position is tracked by the engine. We maintain a local board state.
  const [boardPosition, setBoardPosition] = useState(() => parseFen(INITIAL_FEN));

  // Reset board on new game
  useEffect(() => {
    if (game.moveHistory.length === 0) {
      setBoardPosition(parseFen(game.fen));
      setSelectedSquare(null);
      setValidMoves([]);
    }
  }, [game.fen, game.moveHistory.length]);

  // Apply engine's best move
  useEffect(() => {
    if (bestMove && !game.gameOver && game.currentSide === 'b') {
      const from = uciToSquare(bestMove.move.slice(0, 2));
      const to = uciToSquare(bestMove.move.slice(2, 4));
      if (from && to) {
        setBoardPosition(prev => {
          const next = prev.map(row => [...row]);
          const fromRow = next[from.row];
          const toRow = next[to.row];
          if (fromRow && toRow) {
            toRow[to.col] = fromRow[from.col] ?? null;
            fromRow[from.col] = null;
          }
          return next;
        });
        game.applyEngineMove(bestMove.move);
      }
    }
  }, [bestMove]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleSquareClick = useCallback((square: Square) => {
    if (game.currentSide !== 'w' || game.gameOver) return;

    const piece = boardPosition[square.row]?.[square.col] ?? null;

    // If a piece is selected and clicking a valid move target
    if (selectedSquare && validMoves.some(m => m.row === square.row && m.col === square.col)) {
      // Make the move
      setBoardPosition(prev => {
        const next = prev.map(row => [...row]);
        const fromRow = next[selectedSquare.row];
        const toRow = next[square.row];
        if (fromRow && toRow) {
          toRow[square.col] = fromRow[selectedSquare.col] ?? null;
          fromRow[selectedSquare.col] = null;
        }
        return next;
      });
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

  void depth;
  void position;

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
