import { useReducer, useCallback, useRef } from 'react';

const START_FEN = 'rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1';

type Side = 'w' | 'b';

export interface GameState {
  fen: string;
  moveHistory: string[];
  currentSide: Side;
  gameOver: boolean;
}

type GameAction =
  | { type: 'MOVE'; move: string }
  | { type: 'NEW_GAME' }
  | { type: 'UNDO' }
  | { type: 'SET_POSITION'; fen: string }
  | { type: 'GAME_OVER' };

function flipSide(s: Side): Side { return s === 'w' ? 'b' : 'w'; }

function reducer(state: GameState, action: GameAction): GameState {
  switch (action.type) {
    case 'MOVE':
      return { ...state, moveHistory: [...state.moveHistory, action.move], currentSide: flipSide(state.currentSide) };
    case 'NEW_GAME':
      return { fen: START_FEN, moveHistory: [], currentSide: 'w', gameOver: false };
    case 'UNDO': {
      if (state.moveHistory.length === 0) return state;
      return { ...state, moveHistory: state.moveHistory.slice(0, -1), currentSide: flipSide(state.currentSide) };
    }
    case 'SET_POSITION':
      return { fen: action.fen, moveHistory: [], currentSide: (action.fen.split(' ')[1] as Side) || 'w', gameOver: false };
    case 'GAME_OVER':
      return { ...state, gameOver: true };
  }
}

// Convert board coordinates to UCI move string: file(a-i) + rank(0-9)
// Array row 0 = rank 9 (black back rank), row 9 = rank 0 (red back rank)
function toUci(from: [number, number], to: [number, number]): string {
  const file = (col: number) => String.fromCharCode(97 + col); // a-i
  const rank = (row: number) => 9 - row;
  return `${file(from[0])}${rank(from[1])}${file(to[0])}${rank(to[1])}`;
}

export function useGame(sendCommand: (cmd: string) => void, depth: number = 12, movetime: number = 0) {
  const [state, dispatch] = useReducer(reducer, { fen: START_FEN, moveHistory: [], currentSide: 'w', gameOver: false });
  const depthRef = useRef(depth);
  depthRef.current = depth;
  const movetimeRef = useRef(movetime);
  movetimeRef.current = movetime;

  const makeMove = useCallback((from: [number, number], to: [number, number]) => {
    const move = toUci(from, to);
    dispatch({ type: 'MOVE', move });
    const moves = [...state.moveHistory, move].join(' ');
    sendCommand(`position fen ${state.fen} moves ${moves}`);
    if (movetimeRef.current > 0) {
      sendCommand(`go movetime ${movetimeRef.current}`);
    } else {
      sendCommand(`go depth ${depthRef.current || 12}`);
    }
  }, [state.fen, state.moveHistory, sendCommand]);

  const applyEngineMove = useCallback((move: string) => {
    dispatch({ type: 'MOVE', move });
  }, []);

  const newGame = useCallback(() => {
    dispatch({ type: 'NEW_GAME' });
    sendCommand('ucinewgame');
  }, [sendCommand]);

  const undo = useCallback(() => { dispatch({ type: 'UNDO' }); }, []);

  const setPosition = useCallback((fen: string) => { dispatch({ type: 'SET_POSITION', fen }); }, []);

  const setGameOver = useCallback(() => { dispatch({ type: 'GAME_OVER' }); }, []);

  return { ...state, makeMove, applyEngineMove, newGame, undo, setPosition, setGameOver };
}
