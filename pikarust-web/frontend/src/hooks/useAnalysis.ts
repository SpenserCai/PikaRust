import { useState, useEffect, useCallback } from 'react';
import type { EngineInfo, EngineMessage } from './useEngine';

export interface AnalysisState {
  currentDepth: number;
  score: { cp?: number; mate?: number };
  pv: string;
  nodes: number;
  nps: number;
  wdl: [number, number, number] | null;
  formattedScore: string;
}

function formatScore(score: { cp?: number; mate?: number }): string {
  if (score.mate != null) return `M${score.mate}`;
  if (score.cp != null) return (score.cp >= 0 ? '+' : '') + (score.cp / 100).toFixed(2);
  return '0.00';
}

export function useAnalysis(onMessage: (handler: (msg: EngineMessage) => void) => () => void) {
  const [analysis, setAnalysis] = useState<AnalysisState>({
    currentDepth: 0, score: {}, pv: '', nodes: 0, nps: 0, wdl: null, formattedScore: '0.00',
  });

  const handleMessage = useCallback((msg: EngineMessage) => {
    if (msg.type !== 'info' || !msg.info) return;
    const info: EngineInfo = msg.info;
    setAnalysis((prev) => {
      const score = info.score ?? prev.score;
      return {
        currentDepth: info.depth ?? prev.currentDepth,
        score,
        pv: info.pv ?? prev.pv,
        nodes: info.nodes ?? prev.nodes,
        nps: info.nps ?? prev.nps,
        wdl: info.wdl ?? prev.wdl,
        formattedScore: formatScore(score),
      };
    });
  }, []);

  useEffect(() => onMessage(handleMessage), [onMessage, handleMessage]);

  return analysis;
}
