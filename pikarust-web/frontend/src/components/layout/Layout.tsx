import type { ReactNode } from 'react';

interface Props {
  board: ReactNode;
  sidePanel: ReactNode;
}

export function Layout({ board, sidePanel }: Props) {
  return (
    <div className="min-h-screen bg-[var(--color-bg)] text-[var(--color-text)] p-4 grid grid-cols-1 lg:grid-cols-[1fr_360px] gap-4 items-start">
      <div className="flex items-center justify-center min-h-[60vh] lg:min-h-screen">
        {board}
      </div>
      <aside className="flex flex-col gap-3">
        {sidePanel}
      </aside>
    </div>
  );
}
