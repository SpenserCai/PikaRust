import type { PieceType } from '@/lib/types';

const CHARS: Record<PieceType, string> = {
  K: '帅', A: '仕', B: '相', N: '馬', R: '車', C: '炮', P: '兵',
  k: '将', a: '士', b: '象', n: '马', r: '车', c: '砲', p: '卒',
};

interface PieceProps {
  type: PieceType;
  x: number;
  y: number;
  selected?: boolean;
  isLastMove?: boolean;
  flipped?: boolean;
}

export function Piece({ type, x, y, selected, isLastMove, flipped = false }: PieceProps) {
  const red = type >= 'A' && type <= 'Z';
  const color = red ? 'var(--color-red-piece)' : 'var(--color-black-piece)';

  return (
    <g transform={`translate(${x}, ${y})`} style={{ cursor: 'pointer' }}>
      {selected && (
        <circle r={0.48} fill="none" stroke="var(--color-accent)" strokeWidth={0.08}
          filter="url(#glow)" />
      )}
      <circle r={0.42} fill="var(--color-surface)" stroke={color}
        strokeWidth={isLastMove ? 0.08 : 0.06} />
      <text textAnchor="middle" dy="0.16" fontSize={0.48} fill={color}
        fontWeight="bold" fontFamily="serif" style={{ userSelect: 'none' }}
        transform={flipped ? 'rotate(180)' : undefined}>
        {CHARS[type]}
      </text>
    </g>
  );
}
