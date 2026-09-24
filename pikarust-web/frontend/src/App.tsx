import { useCallback, useState, useMemo } from 'react';
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
import { applyMoveToPosition, findKing, legalTargets } from '@/lib/moves';
import { DEFAULT_SETTINGS, uciToSquare } from '@/lib/game';
import type { Square } from '@/lib/types';

export default function App() {
  const { connected, client, lastInfo, connectionError } = useEngine();
  const [settings, setSettings] = useState(DEFAULT_SETTINGS);
  const game = useGame(client, connected, settings);
  const analysis = useAnalysis(lastInfo);
  const [selection, setSelection] = useState<{ square: Square; revision: number } | null>(null);
  const canPlay = connected && game.phase === 'playing' && game.status === 'ready' && game.currentSide === game.playerSide;
  const selectedSquare = canPlay && selection?.revision === game.revision ? selection.square : null;
  const validMoves = selectedSquare ? legalTargets(game.legalMoves, selectedSquare) : [];

  const boardPosition = useMemo(() => {
    const position = parseFen(game.fen);
    for (const move of game.moveHistory) applyMoveToPosition(position, move);
    return position;
  }, [game.fen, game.moveHistory]);

  const handleSquareClick = useCallback((square: Square) => {
    if (!canPlay) return;
    if (selectedSquare && legalTargets(game.legalMoves, selectedSquare).some(move => move.row === square.row && move.col === square.col)) {
      game.makeMove(selectedSquare, square);
      setSelection(null);
      return;
    }
    const piece = boardPosition[square.row]?.[square.col];
    const ours = piece && (game.playerSide === 'w' ? piece === piece.toUpperCase() : piece === piece.toLowerCase());
    setSelection(ours ? { square, revision: game.revision } : null);
  }, [canPlay, selectedSquare, boardPosition, game]);

  const lastUci = game.moveHistory[game.moveHistory.length - 1];
  const from = lastUci ? uciToSquare(lastUci.slice(0, 2)) : null;
  const to = lastUci ? uciToSquare(lastUci.slice(2)) : null;
  const board = (
    <Board position={boardPosition} onSquareClick={handleSquareClick} selectedSquare={selectedSquare}
      validMoves={validMoves} lastMove={from && to ? { from, to } : null} flipped={game.boardFlipped}
      checkSquare={game.inCheck ? findKing(boardPosition, game.currentSide === 'w' ? 'K' : 'k') : null} />
  );
  const statusBar = (
    <StatusBar currentSide={game.currentSide} playerSide={game.playerSide} phase={game.phase}
      thinking={game.thinking} inCheck={game.inCheck} gameStatus={game.gameStatus}
      status={game.status} connected={connected} error={game.error ?? connectionError} />
  );
  const sidePanel = (
    <>
      <Controls connected={connected} phase={game.phase} playerSide={game.playerSide}
        boardFlipped={game.boardFlipped} canUndo={game.canUndo} settings={settings}
        onStartGame={game.startGame} onNewGame={game.newGame} onUndo={game.undo}
        onSetPlayerSide={game.setPlayerSide} onToggleFlip={game.toggleFlip} onSettingsChange={setSettings}
        canRetry={game.status === 'error'} onRetry={game.retry} />
      <AnalysisPanel analysis={analysis} playerSide={game.playerSide} />
      <MoveHistory moves={game.moveHistory} />
    </>
  );
  return <Layout board={board} sidePanel={sidePanel} statusBar={statusBar} />;
}
