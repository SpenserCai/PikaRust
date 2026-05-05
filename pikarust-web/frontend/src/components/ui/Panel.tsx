import type { ReactNode } from 'react';

interface PanelProps {
  title?: string;
  children: ReactNode;
  className?: string;
}

export function Panel({ title, children, className = '' }: PanelProps) {
  return (
    <div className={`rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)]/80 backdrop-blur-md shadow-[0_0_15px_rgba(0,240,255,0.05)] ${className}`}>
      {title && <h3 className="px-4 pt-3 pb-2 text-xs font-bold uppercase tracking-wider text-[var(--color-accent)]">{title}</h3>}
      <div className="px-4 pb-3">{children}</div>
    </div>
  );
}
