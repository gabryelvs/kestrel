import type { ReactNode } from 'react';

export function Banner({ children }: { children: ReactNode }) {
  return (
    <div className="banner" role="status">
      {children}
    </div>
  );
}
