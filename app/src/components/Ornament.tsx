/** A rule with a small lozenge: the shared divider ornament. */
export function Ornament({ className = "" }: { className?: string }) {
  return (
    <div className={`ornament ${className}`} aria-hidden="true">
      <svg width="14" height="14" viewBox="0 0 14 14">
        <path d="M7 1 13 7 7 13 1 7Z" fill="none" stroke="currentColor" />
        <path d="M7 4.6 9.4 7 7 9.4 4.6 7Z" fill="currentColor" />
      </svg>
    </div>
  );
}
