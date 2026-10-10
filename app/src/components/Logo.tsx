/** The "Hover" mark (spec decision 24). */
export function Logo({ size = 16 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 100 100" fill="var(--accent)" aria-hidden="true">
      <polygon points="50,18 56,28 50,38 44,28" />
      <polygon points="50,42 94,26 88,39 50,57 12,39 6,26" />
      <polygon points="45,61 55,61 60,82 50,77 40,82" />
    </svg>
  );
}
