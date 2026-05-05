const STROKE = 'var(--color-border)';
const STROKE_W = 1.5;

function StarPoint({ x, y }: { x: number; y: number }) {
  const d = 0.15;
  const g = 0.05;
  const segments: [number, number, number, number][] = [];
  if (x > 0) {
    segments.push([x - d, y - g, x - g, y - g], [x - g, y - d, x - g, y - g]);
    segments.push([x - d, y + g, x - g, y + g], [x - g, y + d, x - g, y + g]);
  }
  if (x < 8) {
    segments.push([x + g, y - g, x + d, y - g], [x + g, y - d, x + g, y - g]);
    segments.push([x + g, y + g, x + d, y + g], [x + g, y + d, x + g, y + g]);
  }
  return (
    <g stroke={STROKE} strokeWidth={0.03}>
      {segments.map(([x1, y1, x2, y2], i) => (
        <line key={i} x1={x1} y1={y1} x2={x2} y2={y2} />
      ))}
    </g>
  );
}

export function BoardGrid() {
  const starPoints: [number, number][] = [
    [1, 2], [7, 2], [0, 3], [2, 3], [4, 3], [6, 3], [8, 3],
    [1, 7], [7, 7], [0, 6], [2, 6], [4, 6], [6, 6], [8, 6],
  ];

  return (
    <g>
      {/* Horizontal lines */}
      {Array.from({ length: 10 }, (_, i) => (
        <line key={`h${i}`} x1={0} y1={i} x2={8} y2={i} stroke={STROKE} strokeWidth={STROKE_W} />
      ))}
      {/* Vertical lines - left and right borders full */}
      <line x1={0} y1={0} x2={0} y2={9} stroke={STROKE} strokeWidth={STROKE_W} />
      <line x1={8} y1={0} x2={8} y2={9} stroke={STROKE} strokeWidth={STROKE_W} />
      {/* Inner verticals - broken at river */}
      {Array.from({ length: 7 }, (_, i) => (
        <g key={`vi${i}`}>
          <line x1={i + 1} y1={0} x2={i + 1} y2={4} stroke={STROKE} strokeWidth={STROKE_W} />
          <line x1={i + 1} y1={5} x2={i + 1} y2={9} stroke={STROKE} strokeWidth={STROKE_W} />
        </g>
      ))}

      {/* Palace diagonals */}
      <line x1={3} y1={0} x2={5} y2={2} stroke={STROKE} strokeWidth={STROKE_W} />
      <line x1={5} y1={0} x2={3} y2={2} stroke={STROKE} strokeWidth={STROKE_W} />
      <line x1={3} y1={7} x2={5} y2={9} stroke={STROKE} strokeWidth={STROKE_W} />
      <line x1={5} y1={7} x2={3} y2={9} stroke={STROKE} strokeWidth={STROKE_W} />

      {/* River text */}
      <text x={4} y={4.6} textAnchor="middle" fontSize={0.5} fill="var(--color-text-dim)" fontFamily="serif" opacity={0.6}>
        {"楚 河\u3000\u3000\u3000漢 界"}
      </text>

      {/* Star points */}
      {starPoints.map(([x, y]) => <StarPoint key={`${x},${y}`} x={x} y={y} />)}
    </g>
  );
}
