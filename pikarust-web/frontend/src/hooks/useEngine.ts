import { useState, useRef, useCallback, useEffect } from 'react';

export interface EngineInfo {
  depth?: number;
  score?: { cp?: number; mate?: number };
  nodes?: number;
  nps?: number;
  pv?: string;
  wdl?: [number, number, number];
}

type MessageType = 'uciok' | 'readyok' | 'info' | 'bestmove' | 'unknown';
export interface EngineMessage {
  type: MessageType;
  raw: string;
  info?: EngineInfo;
  bestMove?: string;
}

export interface BestMoveEvent {
  move: string;
  id: number;
}

type MessageHandler = (msg: EngineMessage) => void;

function parseInfo(line: string): EngineInfo {
  const info: EngineInfo = {};
  const tokens = line.split(' ');
  for (let i = 0; i < tokens.length; i++) {
    switch (tokens[i]) {
      case 'depth':
        info.depth = Number(tokens[++i]);
        break;
      case 'score':
        if (tokens[i + 1] === 'cp') { info.score = { cp: Number(tokens[i += 2]) }; }
        else if (tokens[i + 1] === 'mate') { info.score = { mate: Number(tokens[i += 2]) }; }
        break;
      case 'nodes':
        info.nodes = Number(tokens[++i]);
        break;
      case 'nps':
        info.nps = Number(tokens[++i]);
        break;
      case 'pv':
        info.pv = tokens.slice(i + 1).join(' ');
        i = tokens.length;
        break;
      case 'wdl':
        info.wdl = [Number(tokens[i + 1]), Number(tokens[i + 2]), Number(tokens[i + 3])];
        i += 3;
        break;
    }
  }
  return info;
}

function categorize(line: string): EngineMessage {
  if (line === 'uciok') return { type: 'uciok', raw: line };
  if (line === 'readyok') return { type: 'readyok', raw: line };
  if (line.startsWith('info ')) return { type: 'info', raw: line, info: parseInfo(line) };
  if (line.startsWith('bestmove')) return { type: 'bestmove', raw: line, bestMove: line.split(' ')[1] };
  return { type: 'unknown', raw: line };
}

export function useEngine() {
  const [connected, setConnected] = useState(false);
  const [lastInfo, setLastInfo] = useState<EngineInfo | null>(null);
  const [bestMove, setBestMove] = useState<BestMoveEvent | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const handlersRef = useRef<Set<MessageHandler>>(new Set());
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const bestMoveIdRef = useRef(0);

  const connect = useCallback(() => {
    const url = import.meta.env.PROD ? `ws://${window.location.host}/ws` : 'ws://localhost:9000/ws';
    const ws = new WebSocket(url);
    wsRef.current = ws;

    ws.onopen = () => setConnected(true);
    ws.onclose = () => {
      setConnected(false);
      reconnectTimer.current = setTimeout(connect, 2000);
    };
    ws.onmessage = (e: MessageEvent<string>) => {
      for (const line of e.data.split('\n')) {
        if (!line.trim()) continue;
        const msg = categorize(line.trim());
        if (msg.type === 'info') setLastInfo(msg.info ?? null);
        if (msg.type === 'bestmove') {
          bestMoveIdRef.current += 1;
          setBestMove(msg.bestMove ? { move: msg.bestMove, id: bestMoveIdRef.current } : null);
        }
        if (msg.type === 'uciok') {
          ws.send('setoption name UCI_ShowWDL value true');
          ws.send('isready');
        }
        handlersRef.current.forEach((h) => h(msg));
      }
    };
  }, []);

  useEffect(() => {
    connect();
    return () => {
      clearTimeout(reconnectTimer.current);
      wsRef.current?.close();
    };
  }, [connect]);

  const sendCommand = useCallback((cmd: string) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(cmd);
    }
  }, []);

  const onMessage = useCallback((handler: MessageHandler) => {
    handlersRef.current.add(handler);
    return () => { handlersRef.current.delete(handler); };
  }, []);

  return { connected, sendCommand, onMessage, lastInfo, bestMove };
}
