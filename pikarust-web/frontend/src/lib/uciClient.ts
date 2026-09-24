import { searchCommand } from './game.ts';
import type { GameStatus, SearchSettings } from './game.ts';

export interface EngineInfo {
  depth?: number;
  score?: { cp?: number; mate?: number };
  nodes?: number;
  nps?: number;
  pv?: string;
  wdl?: [number, number, number];
}
export interface PositionInfo { legalMoves: string[]; inCheck: boolean; gameStatus: GameStatus }
export class CancelledRequest extends Error {
  constructor() { super('Engine request cancelled'); this.name = 'CancelledRequest'; }
}

export function parseInfo(line: string): EngineInfo {
  const info: EngineInfo = {};
  const tokens = line.trim().split(/\s+/);
  for (let i = 1; i < tokens.length; i++) {
    const token = tokens[i];
    const value = Number(tokens[i + 1]);
    if (token === 'depth' || token === 'nodes' || token === 'nps') {
      if (Number.isFinite(value)) info[token] = value;
      i++;
    } else if (token === 'score') {
      const kind = tokens[i + 1];
      const score = Number(tokens[i + 2]);
      if ((kind === 'cp' || kind === 'mate') && Number.isFinite(score)) info.score = { [kind]: score };
      i += 2;
    } else if (token === 'wdl') {
      const values = tokens.slice(i + 1, i + 4).map(Number);
      if (values.length === 3 && values.every(Number.isFinite)) info.wdl = values as [number, number, number];
      i += 3;
    } else if (token === 'pv') {
      info.pv = tokens.slice(i + 1).join(' ');
      break;
    }
  }
  return info;
}

interface Operation {
  kind: 'inspect' | 'search';
  commands: string[];
  resolve: (value: PositionInfo | string) => void;
  reject: (reason: Error) => void;
  cancelled: boolean;
  error: Error | null;
  barrierSent: boolean;
  moves: string[];
  nodeCount: number | null;
  inCheck: boolean | null;
  gameStatus: GameStatus | null;
  bestMove: string | null;
  timeoutMs: number;
  timer?: ReturnType<typeof setTimeout>;
}
interface Callbacks {
  ready: (ready: boolean) => void;
  info: (info: EngineInfo | null) => void;
  failure: (error: Error) => void;
}

/** Serial UCI transactions, delimited by readyok after all result-producing work.
 * A cancelled search stays in flight until stop + readyok drain its output. */
export class UciClient {
  private sendRaw: ((command: string) => void) | null = null;
  private phase: 'disconnected' | 'uci' | 'ready' | 'connected' = 'disconnected';
  private active: Operation | null = null;
  private queue: Operation[] = [];
  private callbacks: Callbacks;
  private handshakeTimer?: ReturnType<typeof setTimeout>;
  private info: EngineInfo | null = null;

  constructor(callbacks: Callbacks) { this.callbacks = callbacks; }

  attach(send: (command: string) => void): void {
    this.disconnect();
    this.sendRaw = send;
    this.phase = 'uci';
    // The bridge sends `uci` when it launches the child process.
    this.handshakeTimer = setTimeout(() => this.fail(new Error('引擎初始化超时。')), 60_000);
  }

  disconnect(): void {
    clearTimeout(this.handshakeTimer);
    this.sendRaw = null;
    this.phase = 'disconnected';
    if (this.active) {
      clearTimeout(this.active.timer);
      this.active.reject(new CancelledRequest());
      this.active = null;
    }
    for (const operation of this.queue.splice(0)) operation.reject(new CancelledRequest());
    this.clearInfo();
    this.callbacks.ready(false);
  }

  clearInfo(): void { this.info = null; this.callbacks.info(null); }

  cancel(): void {
    for (const operation of this.queue.splice(0)) operation.reject(new CancelledRequest());
    const operation = this.active;
    if (!operation || operation.cancelled) return;
    operation.cancelled = true;
    operation.reject(new CancelledRequest());
    if (operation.kind === 'search' && !operation.barrierSent) {
      operation.barrierSent = true;
      this.sendRaw?.('stop');
      this.sendRaw?.('isready');
    }
  }

  inspect(position: string, newGame = false): Promise<PositionInfo> {
    const commands = [...(newGame ? ['ucinewgame'] : []), position, 'go perft 1', 'eval', 'd', 'isready'];
    return this.enqueue('inspect', commands, 15_000) as Promise<PositionInfo>;
  }

  search(position: string, settings: SearchSettings): Promise<string> {
    const timeout = settings.mode === 'movetime' ? settings.movetime + 15_000 : 300_000;
    return this.enqueue('search', [position, searchCommand(settings)], timeout) as Promise<string>;
  }

  accept(raw: string): void {
    const line = raw.trim();
    if (this.phase === 'uci' && line === 'uciok') {
      this.phase = 'ready';
      this.sendRaw?.('setoption name UCI_ShowWDL value true');
      this.sendRaw?.('isready');
      return;
    }
    if (this.phase === 'ready' && line === 'readyok') {
      clearTimeout(this.handshakeTimer);
      this.phase = 'connected';
      this.callbacks.ready(true);
      this.pump();
      return;
    }
    const operation = this.active;
    if (!operation) return;
    if (line.startsWith('info string error:')) {
      operation.error = new Error(line.slice('info string error:'.length).trim());
      if (operation.kind === 'search' && !operation.barrierSent) {
        operation.barrierSent = true;
        this.sendRaw?.('stop');
        this.sendRaw?.('isready');
      }
    } else if (operation.kind === 'inspect') {
      const move = /^([a-i][0-9][a-i][0-9]):\s+(\d+)$/.exec(line);
      const status = /^Game status: (ongoing|draw|win|loss)$/.exec(line);
      const count = /^Nodes searched:\s+(\d+)$/.exec(line);
      if (move) {
        if (Number(move[2]) !== 1) operation.error = new Error('引擎着法响应不完整。');
        operation.moves.push(move[1]!);
      } else if (count) operation.nodeCount = Number(count[1]);
      else if (status) operation.gameStatus = status[1] as GameStatus;
      else if (line === 'Final evaluation: none (in check)') operation.inCheck = true;
      else if (/^Final evaluation [+-]?\d+ \(side to move, internal units\)$/.test(line)) operation.inCheck = false;
    } else if (line.startsWith('bestmove ')) {
      operation.bestMove = line.split(/\s+/)[1] ?? null;
      if (!operation.barrierSent) {
        operation.barrierSent = true;
        this.sendRaw?.('isready');
      }
    } else if (line.startsWith('info ') && !line.startsWith('info string ') && !operation.cancelled) {
      this.info = { ...this.info, ...parseInfo(line) };
      this.callbacks.info(this.info);
    }
    if (line === 'readyok' && operation.barrierSent) this.complete(operation);
  }

  private enqueue(kind: Operation['kind'], commands: string[], timeoutMs: number): Promise<PositionInfo | string> {
    if (this.phase !== 'connected') return Promise.reject(new Error('引擎尚未连接。'));
    return new Promise((resolve, reject) => {
      this.queue.push({ kind, commands, resolve, reject, cancelled: false, error: null,
        barrierSent: kind === 'inspect', moves: [], nodeCount: null, inCheck: null, gameStatus: null, bestMove: null, timeoutMs });
      this.pump();
    });
  }

  private pump(): void {
    if (this.active || this.phase !== 'connected') return;
    const operation = this.queue.shift();
    if (!operation) return;
    this.active = operation;
    if (operation.kind === 'search') this.clearInfo();
    operation.timer = setTimeout(() => this.fail(new Error('引擎响应超时，请重新连接。')), operation.timeoutMs);
    for (const command of operation.commands) this.sendRaw?.(command);
  }

  private complete(operation: Operation): void {
    clearTimeout(operation.timer);
    this.active = null;
    if (!operation.cancelled) {
      if (operation.error) operation.reject(operation.error);
      else if (operation.kind === 'inspect') {
        const legalMoves = [...new Set(operation.moves)];
        if (operation.nodeCount !== legalMoves.length || operation.inCheck === null || operation.gameStatus === null
          || (operation.gameStatus === 'ongoing' && legalMoves.length === 0)) {
          operation.reject(new Error('引擎没有返回完整的局面信息。'));
        } else operation.resolve({ legalMoves, inCheck: operation.inCheck, gameStatus: operation.gameStatus });
      } else if (operation.bestMove) operation.resolve(operation.bestMove);
      else operation.reject(new Error('引擎没有返回着法。'));
    }
    this.pump();
  }

  private fail(error: Error): void {
    this.sendRaw?.('stop');
    const active = this.active;
    if (active && !active.cancelled) active.reject(error);
    this.disconnect();
    this.callbacks.failure(error);
  }
}
