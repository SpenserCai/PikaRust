import type { EngineInfo } from '@/lib/uciClient';

export interface AnalysisState {
  currentDepth: number;
  score: { cp?: number; mate?: number };
  pv: string;
  nodes: number;
  nps: number;
  wdl: [number, number, number] | null;
}

/** Analysis belongs to the client's current search, not a global UCI stream. */
export function useAnalysis(info: EngineInfo | null): AnalysisState {
  return {
    currentDepth: info?.depth ?? 0, score: info?.score ?? {}, pv: info?.pv ?? '',
    nodes: info?.nodes ?? 0, nps: info?.nps ?? 0, wdl: info?.wdl ?? null,
  };
}
