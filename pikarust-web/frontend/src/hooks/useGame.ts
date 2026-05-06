import { useReducer, useCallback, useRef } from 'react';

const START_FEN = 'rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1';

export type Side = 'w' | 'b';
export type Phase = 'idle' | 'playing' | 'ended';

export interface GameState {
  fen: string;
  moveHistory: string[];
  currentSide: Side;
  playerSide: Side;
  boardFlipped: boolean;
  phase: Phase;
  gameOver: boolean;
  thinking: boolean;
}

type GameAction =
  | { type: 'MOVE'; move: string }
  | { type: 'NEW_GAME' }
  | { type: 'UNDO' }
  | { type: 'SET_POSITION'; fen: string }
  | { type: 'GAME_OVER' }
  | { type: 'SET_THINKING'; thinking: boolean }
  | { type: 'SET_PLAYER_SIDE'; side: Side }
  | { type: 'START_GAME' }
  | { type: 'TOGGLE_FLIP' };

function flipSide(s: Side): Side { return s === 'w' ? 'b' : 'w'; }

const initialState: GameState = {
  fen: START_FEN, moveHistory: [], currentSide: 'w',
  playerSide: 'w', boardFlipped: false, phase: 'idle', gameOver: false, thinking: false,
};

function reducer(state: GameState, action: GameAction): GameState {
  switch (action.type) {
    case 'MOVE':
      return { ...state, moveHistory: [...state.moveHistory, action.move], currentSide: flipSide(state.currentSide), thinking: false };
    case 'NEW_GAME':
      return { ...initialState };
    case 'UNDO': {
      const count = state.moveHistory.length >= 2 ? 2 : state.moveHistory.length;
      if (count === 0) return state;
      const newHistory = state.moveHistory.slice(0, -count);
      const newSide = count % 2 === 0 ? state.currentSide : flipSide(state.currentSide);
      return { ...state, moveHistory: newHistory, currentSide: newSide, thinking: false };
    }
    case 'SET_POSITION':
      return { ...state, fen: action.fen, moveHistory: [], currentSide: (action.fen.split(' ')[1] as Side) || 'w', gameOver: false, thinking: false };
    case 'GAME_OVER':
      return { ...state, phase: 'ended', gameOver: true, thinking: false };
    case 'SET_THINKING':
      return { ...state, thinking: action.thinking };
    case 'SET_PLAYER_SIDE':
      return state.phase === 'idle' ? { ...state, playerSide: action.side, boardFlipped: action.side === 'b' } : state;
    case 'START_GAME':
      return state.phase === 'idle' ? { ...state, phase: 'playing' } : state;
    case 'TOGGLE_FLIP':
      return { ...state, boardFlipped: !state.boardFlipped };
  }
}

function toUci(from: [number, number], to: [number, number]): string {
  const file = (col: number) => String.fromCharCode(97 + col);
  const rank = (row: number) => 9 - row;
  return `${file(from[0])}${rank(from[1])}${file(to[0])}${rank(to[1])}`;
}

export function useGame(sendCommand: (cmd: string) => void, depth: number = 12, movetime: number = 0) {
  const [state, dispatch] = useReducer(reducer, initialState);
  const depthRef = useRef(depth);
  depthRef.current = depth;
  const movetimeRef = useRef(movetime);
  movetimeRef.current = movetime;

  const requestEngineMove = useCallback((moves: string[]) => {
    dispatch({ type: 'SET_THINKING', thinking: true });
    const movesStr = moves.length > 0 ? ` moves ${moves.join(' ')}` : '';
    sendCommand(`position fen ${state.fen}${movesStr}`);
    if (movetimeRef.current > 0) {
      sendCommand(`go movetime ${movetimeRef.current}`);
    } else {
      sendCommand(`go depth ${depthRef.current || 12}`);
    }
  }, [state.fen, sendCommand]);

  const makeMove = useCallback((from: [number, number], to: [number, number]) => {
    const move = toUci(from, to);
    dispatch({ type: 'MOVE', move });
    const moves = [...state.moveHistory, move];
    requestEngineMove(moves);
  }, [state.moveHistory, requestEngineMove]);

  const applyEngineMove = useCallback((move: string) => {
    dispatch({ type: 'MOVE', move });
  }, []);

  const startGame = useCallback(() => {
    dispatch({ type: 'START_GAME' });
    sendCommand('ucinewgame');
    sendCommand('setoption name UCI_ShowWDL value true');
    // If player is black, engine moves first
    if (state.playerSide === 'b') {
      requestEngineMove([]);
    }
  }, [sendCommand, state.playerSide, requestEngineMove]);

  const newGame = useCallback(() => {
    dispatch({ type: 'NEW_GAME' });
  }, []);

  const undo = useCallback(() => {
    dispatch({ type: 'UNDO' });
  }, []);

  const setPlayerSide = useCallback((side: Side) => {
    dispatch({ type: 'SET_PLAYER_SIDE', side });
  }, []);

  const toggleFlip = useCallback(() => {
    dispatch({ type: 'TOGGLE_FLIP' });
  }, []);

  const setPosition = useCallback((fen: string) => { dispatch({ type: 'SET_POSITION', fen }); }, []);
  const setGameOver = useCallback(() => { dispatch({ type: 'GAME_OVER' }); }, []);

  return { ...state, makeMove, applyEngineMove, startGame, newGame, undo, setPlayerSide, toggleFlip, setPosition, setGameOver };
}
