import { INITIAL_FEN } from './fen.ts';
import type { Side, Square } from './types.ts';

export type Phase = 'idle' | 'playing' | 'ended';
/** Native engine adjudication; win/loss are relative to the side to move. */
export type GameStatus = 'ongoing' | 'draw' | 'win' | 'loss';
export type TurnStatus = 'idle' | 'syncing' | 'ready' | 'thinking' | 'disconnected' | 'error';
export interface SearchSettings {
  mode: 'depth' | 'movetime';
  depth: number;
  movetime: number;
}
export const DEFAULT_SETTINGS: SearchSettings = { mode: 'depth', depth: 20, movetime: 3000 };

export interface GameState {
  fen: string;
  moveHistory: string[];
  currentSide: Side;
  playerSide: Side;
  boardFlipped: boolean;
  phase: Phase;
  status: TurnStatus;
  revision: number;
  legalMoves: string[];
  inCheck: boolean;
  gameStatus: GameStatus;
  error: string | null;
}

export type GameAction =
  | { type: 'PLAYER_MOVE'; move: string }
  | { type: 'ENGINE_MOVE'; revision: number; move: string }
  | { type: 'INSPECTED'; revision: number; legalMoves: string[]; inCheck: boolean; gameStatus: GameStatus }
  | { type: 'SYNC'; revision: number }
  | { type: 'ERROR'; revision: number; message: string }
  | { type: 'DISCONNECTED' }
  | { type: 'NEW_GAME' }
  | { type: 'UNDO' }
  | { type: 'SET_POSITION'; fen: string }
  | { type: 'SET_PLAYER_SIDE'; side: Side }
  | { type: 'START_GAME' }
  | { type: 'RETRY' }
  | { type: 'TOGGLE_FLIP' };

export function flipSide(side: Side): Side { return side === 'w' ? 'b' : 'w'; }
export function initialSide(fen: string): Side { return fen.trim().split(/\s+/)[1] === 'b' ? 'b' : 'w'; }
export function createGameState(): GameState {
  return {
    fen: INITIAL_FEN, moveHistory: [], currentSide: 'w', playerSide: 'w',
    boardFlipped: false, phase: 'idle', status: 'idle', revision: 0,
    legalMoves: [], inCheck: false, gameStatus: 'ongoing', error: null,
  };
}

/** Rewind to the position before the most recent human decision. */
export function undoTarget(state: GameState): number | null {
  const first = initialSide(state.fen);
  for (let index = state.moveHistory.length - 1; index >= 0; index--) {
    const mover = index % 2 === 0 ? first : flipSide(first);
    if (mover === state.playerSide) return index;
  }
  return null;
}

function moved(state: GameState, move: string): GameState {
  return {
    ...state, moveHistory: [...state.moveHistory, move], currentSide: flipSide(state.currentSide),
    status: 'syncing', revision: state.revision + 1, legalMoves: [], inCheck: false, gameStatus: 'ongoing', error: null,
  };
}

export function gameReducer(state: GameState, action: GameAction): GameState {
  switch (action.type) {
    case 'PLAYER_MOVE':
      return state.phase === 'playing' && state.status === 'ready' && state.currentSide === state.playerSide
        && state.legalMoves.includes(action.move) ? moved(state, action.move) : state;
    case 'ENGINE_MOVE':
      if (action.revision !== state.revision || state.phase !== 'playing' || state.status !== 'thinking'
        || state.currentSide === state.playerSide) return state;
      if (!state.legalMoves.includes(action.move)) {
        return { ...state, status: 'error', error: '引擎返回了无效着法，请重试或开始新对局。' };
      }
      return moved(state, action.move);
    case 'INSPECTED':
      if (action.revision !== state.revision || state.phase !== 'playing') return state;
      return {
        ...state, legalMoves: action.legalMoves, inCheck: action.inCheck, gameStatus: action.gameStatus, error: null,
        phase: action.gameStatus === 'ongoing' ? 'playing' : 'ended',
        status: action.gameStatus !== 'ongoing' ? 'idle' : state.currentSide === state.playerSide ? 'ready' : 'thinking',
      };
    case 'SYNC':
      return action.revision === state.revision && state.phase === 'playing'
        ? { ...state, status: 'syncing', legalMoves: [], error: null } : state;
    case 'ERROR':
      return action.revision === state.revision && state.phase === 'playing'
        ? { ...state, status: 'error', error: action.message, legalMoves: [] } : state;
    case 'DISCONNECTED':
      return { ...state, status: state.phase === 'playing' ? 'disconnected' : 'idle', legalMoves: [], error: null };
    case 'NEW_GAME':
      return { ...createGameState(), playerSide: state.playerSide, boardFlipped: state.boardFlipped, revision: state.revision + 1 };
    case 'UNDO': {
      const target = undoTarget(state);
      if (target === null) return state;
      return {
        ...state, moveHistory: state.moveHistory.slice(0, target), currentSide: state.playerSide,
        phase: 'playing', status: 'syncing', revision: state.revision + 1,
        legalMoves: [], inCheck: false, gameStatus: 'ongoing', error: null,
      };
    }
    case 'SET_POSITION':
      return {
        ...state, fen: action.fen, moveHistory: [], currentSide: initialSide(action.fen),
        phase: 'idle', status: 'idle', revision: state.revision + 1,
        legalMoves: [], inCheck: false, gameStatus: 'ongoing', error: null,
      };
    case 'SET_PLAYER_SIDE':
      return state.phase === 'idle' ? { ...state, playerSide: action.side, boardFlipped: action.side === 'b' } : state;
    case 'START_GAME':
      return state.phase === 'idle' ? { ...state, phase: 'playing', status: 'syncing', revision: state.revision + 1 } : state;
    case 'RETRY':
      return state.phase === 'playing' ? { ...state, revision: state.revision + 1, status: 'syncing', error: null } : state;
    case 'TOGGLE_FLIP':
      return { ...state, boardFlipped: !state.boardFlipped };
  }
}

export function squareToUci(square: Square): string {
  return `${String.fromCharCode(97 + square.col)}${9 - square.row}`;
}

export function uciToSquare(value: string): Square | null {
  if (!/^[a-i][0-9]$/.test(value)) return null;
  return { col: value.charCodeAt(0) - 97, row: 9 - Number(value[1]) };
}

export function positionCommand(fen: string, moves: string[]): string {
  return `position fen ${fen}${moves.length ? ` moves ${moves.join(' ')}` : ''}`;
}

export function searchCommand(settings: SearchSettings): string {
  return settings.mode === 'movetime' ? `go movetime ${settings.movetime}` : `go depth ${settings.depth}`;
}
