import type { ReactNode } from 'react';

interface Props {
  board: ReactNode;
  sidePanel: ReactNode;
  statusBar?: ReactNode;
}

export function Layout({ board, sidePanel, statusBar }: Props) {
  return (
    <div className="min-h-screen bg-[var(--color-bg)] text-[var(--color-text)] p-3 lg:p-6 grid grid-cols-1 lg:grid-cols-[1fr_340px] gap-4 lg:gap-6 items-start">
      <div className="flex flex-col items-center justify-center gap-2 lg:min-h-screen">
        {board}
        {statusBar}
      </div>
      <aside className="flex flex-col gap-3 lg:sticky lg:top-6 lg:max-h-[calc(100vh-3rem)] lg:overflow-y-auto">
        {sidePanel}
      </aside>
    </div>
  );
}
