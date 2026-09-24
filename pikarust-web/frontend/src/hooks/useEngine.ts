import { useState, useRef, useCallback, useEffect } from 'react';
import { UciClient } from '@/lib/uciClient';
import type { EngineInfo } from '@/lib/uciClient';

export function useEngine() {
  const [connected, setConnected] = useState(false);
  const [lastInfo, setLastInfo] = useState<EngineInfo | null>(null);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const [client] = useState(() => new UciClient({
    ready: setConnected,
    info: setLastInfo,
    failure: (error) => {
      setConnectionError(error.message);
      wsRef.current?.close();
    },
  }));

  const connect = useCallback(function openConnection() {
    const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws';
    const url = import.meta.env.PROD ? `${scheme}://${window.location.host}/ws` : 'ws://localhost:9000/ws';
    const ws = new WebSocket(url);
    wsRef.current = ws;
    ws.onopen = () => {
      if (wsRef.current !== ws) return;
      setConnectionError(null);
      client.attach((command) => ws.send(command));
    };
    ws.onclose = () => {
      if (wsRef.current !== ws) return;
      wsRef.current = null;
      client.disconnect();
      setConnectionError('连接中断，正在重新连接…');
      reconnectTimer.current = setTimeout(openConnection, 2000);
    };
    ws.onerror = () => ws.close();
    ws.onmessage = (event: MessageEvent<string>) => {
      if (wsRef.current !== ws) return;
      for (const line of event.data.split('\n')) {
        if (line.trim()) client.accept(line);
      }
    };
  }, [client]);

  useEffect(() => {
    connect();
    return () => {
      clearTimeout(reconnectTimer.current);
      const ws = wsRef.current;
      wsRef.current = null;
      client.disconnect();
      ws?.close();
    };
  }, [connect, client]);

  return { connected, client, lastInfo, connectionError };
}
