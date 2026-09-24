import { useReducer, useCallback, useEffect } from 'react';
import { createGameState, gameReducer, positionCommand, squareToUci, undoTarget } from '@/lib/game';
import { CancelledRequest } from '@/lib/uciClient';
import type { SearchSettings } from '@/lib/game';
import type { UciClient } from '@/lib/uciClient';
import type { Side, Square } from '@/lib/types';
export type { Phase } from '@/lib/game';
export type { Side } from '@/lib/types';

export function useGame(client: UciClient, connected: boolean, settings: SearchSettings) {
  const [state, dispatch] = useReducer(gameReducer, undefined, createGameState);
  const { fen, moveHistory, currentSide, playerSide, phase, revision } = state;
  const { mode, depth, movetime } = settings;

  useEffect(() => {
    if (!connected) {
      dispatch({ type: 'DISCONNECTED' });
      return;
    }
    if (phase !== 'playing') return;
    let current = true;
    dispatch({ type: 'SYNC', revision });
    const position = positionCommand(fen, moveHistory);
    async function synchronizeTurn() {
      try {
        const inspected = await client.inspect(position, moveHistory.length === 0);
        if (!current) return;
        dispatch({ type: 'INSPECTED', revision, ...inspected });
        if (inspected.gameStatus !== 'ongoing' || currentSide === playerSide) return;
        const move = await client.search(position, { mode, depth, movetime });
        if (current) dispatch({ type: 'ENGINE_MOVE', revision, move });
      } catch (error) {
        if (current && !(error instanceof CancelledRequest)) {
          dispatch({ type: 'ERROR', revision, message: error instanceof Error ? error.message : '引擎请求失败。' });
        }
      }
    }
    void synchronizeTurn();
    return () => {
      current = false;
      client.cancel();
    };
  }, [client, connected, fen, moveHistory, currentSide, playerSide, phase, revision, mode, depth, movetime]);

  const makeMove = useCallback((from: Square, to: Square) => {
    if (connected) dispatch({ type: 'PLAYER_MOVE', move: squareToUci(from) + squareToUci(to) });
  }, [connected]);
  const startGame = useCallback(() => {
    if (!connected) return;
    client.clearInfo();
    dispatch({ type: 'START_GAME' });
  }, [client, connected]);
  const newGame = useCallback(() => {
    client.cancel();
    client.clearInfo();
    dispatch({ type: 'NEW_GAME' });
  }, [client]);
  const canUndo = undoTarget(state) !== null;
  const undo = useCallback(() => {
    if (!canUndo) return;
    client.cancel();
    client.clearInfo();
    dispatch({ type: 'UNDO' });
  }, [client, canUndo]);
  const setPosition = useCallback((position: string) => {
    client.cancel();
    client.clearInfo();
    dispatch({ type: 'SET_POSITION', fen: position });
  }, [client]);
  const setPlayerSide = useCallback((side: Side) => dispatch({ type: 'SET_PLAYER_SIDE', side }), []);
  const toggleFlip = useCallback(() => dispatch({ type: 'TOGGLE_FLIP' }), []);
  const retry = useCallback(() => dispatch({ type: 'RETRY' }), []);

  return {
    ...state, thinking: state.status === 'thinking', canUndo,
    makeMove, startGame, newGame, undo, setPlayerSide, toggleFlip, setPosition, retry,
  };
}
