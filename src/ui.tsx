// Lightweight Calm primitives — token-based, no radix dependency (a one-screen
// console doesn't need the full shadcn tree; matching the ecosystem tokens is
// what keeps it visually consistent).
import type { ReactNode } from "react";

export function cn(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}

export function Card({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "rounded-xl border border-border bg-card text-card-foreground shadow-sm",
        className,
      )}
    >
      {children}
    </div>
  );
}

export function Stat({
  label,
  value,
  hint,
  accent,
}: {
  label: string;
  value: ReactNode;
  hint?: ReactNode;
  accent?: string;
}) {
  return (
    <Card className="px-4 py-3">
      <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        {label}
      </p>
      <p className={cn("mt-1 text-2xl font-semibold tabular-nums", accent)}>
        {value}
      </p>
      {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
    </Card>
  );
}

/** A small colored status dot using a semantic token color. */
export function Dot({ className }: { className?: string }) {
  return (
    <span
      className={cn("inline-block size-2 rounded-full", className)}
      aria-hidden
    />
  );
}
