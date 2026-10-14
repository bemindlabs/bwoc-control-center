import { useState } from "react";
import type { ControllerInfo } from "./types";
import { cn } from "./ui";
import { HubMark } from "./Home";

/** A signed-request glyph: a key whose teeth resolve into a checkmark — "this
 *  device proves itself, an operator approves it once". */
function KeyMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 100 100" className={className} fill="none">
      <circle cx="36" cy="40" r="18" stroke="currentColor" strokeWidth="6" />
      <circle cx="36" cy="40" r="6" fill="currentColor" />
      <path
        d="M48 52L70 74"
        stroke="currentColor"
        strokeWidth="6"
        strokeLinecap="round"
      />
      <path
        d="M62 66L74 54"
        stroke="currentColor"
        strokeWidth="6"
        strokeLinecap="round"
      />
      <path
        d="M58 80l8 8 16-18"
        stroke="currentColor"
        strokeOpacity="0.5"
        strokeWidth="5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** A tailnet glyph: two devices linked by an encrypted path over the mesh. */
function MeshMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 100 100" className={className} fill="none">
      <path
        d="M26 50C26 36 38 30 50 30s24 6 24 20"
        stroke="currentColor"
        strokeOpacity="0.3"
        strokeWidth="3"
        strokeDasharray="4 6"
        strokeLinecap="round"
      />
      <rect x="14" y="44" width="22" height="30" rx="4" stroke="currentColor" strokeWidth="5" />
      <rect x="64" y="44" width="22" height="30" rx="4" stroke="currentColor" strokeWidth="5" />
      <circle cx="25" cy="69" r="2.5" fill="currentColor" />
      <circle cx="75" cy="69" r="2.5" fill="currentColor" />
      <path d="M36 59h28" stroke="currentColor" strokeWidth="5" strokeLinecap="round" />
      <circle cx="50" cy="59" r="7" fill="currentColor" opacity="0.18" />
      <circle cx="50" cy="59" r="3.5" fill="currentColor" />
    </svg>
  );
}

const STEPS = [
  {
    Mark: HubMark,
    title: "BWOC Control Center",
    body: "Watch and chat with your agent fleet — from anywhere on your private mesh.",
  },
  {
    Mark: KeyMark,
    title: "Secure by design",
    body: "This device signs every request with its own key. A host operator approves it once — no public ports, everything rides the tailnet.",
  },
  {
    Mark: MeshMark,
    title: "You're set",
    body: "Open a host and the app requests access automatically. Approve it on that host, and the fleet appears.",
  },
] as const;

export default function Onboarding({
  controller,
  onDone,
}: {
  controller: ControllerInfo | null;
  onDone: () => void;
}) {
  const [i, setI] = useState(0);
  const last = i === STEPS.length - 1;
  const step = STEPS[i];
  const Mark = step.Mark;

  return (
    <main
      className="flex h-[100dvh] flex-col overflow-hidden bg-background text-foreground"
      style={{
        paddingTop: "env(safe-area-inset-top)",
        paddingBottom: "calc(env(safe-area-inset-bottom) + 1rem)",
      }}
    >
      <div className="flex justify-end px-5 pt-3">
        <button
          onClick={onDone}
          // Hidden on the last step — also keep it out of the tab order.
          disabled={last}
          aria-hidden={last}
          className={cn(
            "text-sm text-muted-foreground transition-opacity hover:text-foreground",
            last && "pointer-events-none opacity-0",
          )}
        >
          Skip
        </button>
      </div>

      <div className="flex flex-1 flex-col items-center justify-center gap-6 px-8 text-center">
        {/* art: branded glyph on a teal glow — remounts per step for a soft
            fade/rise entrance */}
        <div key={i} className="relative grid place-items-center animate-[fadeRise_.45s_ease-out]">
          <div
            aria-hidden
            className="pointer-events-none absolute size-52 rounded-full bg-primary/20 blur-[60px]"
          />
          <div className="grid size-28 place-items-center rounded-[2rem] bg-gradient-to-b from-card to-background text-primary shadow-xl ring-1 ring-primary/25">
            <Mark className="size-16" />
          </div>
        </div>

        <div key={`t${i}`} className="flex flex-col items-center gap-3 animate-[fadeRise_.45s_ease-out_.05s_both]">
          <h1 className="bg-gradient-to-b from-foreground to-foreground/70 bg-clip-text text-2xl font-bold text-transparent">
            {step.title}
          </h1>
          <p className="max-w-xs text-sm leading-relaxed text-muted-foreground">
            {step.body}
          </p>
          {last && controller && (
            <p className="mt-1 rounded-lg border border-border bg-card px-3 py-2 text-xs text-muted-foreground">
              this device · <span className="font-mono">{controller.id}</span>
            </p>
          )}
        </div>
      </div>

      <div className="flex flex-col gap-4 px-8">
        <div className="flex justify-center gap-1.5">
          {STEPS.map((_, n) => (
            <button
              key={n}
              onClick={() => setI(n)}
              aria-label={`step ${n + 1}`}
              className={cn(
                "h-1.5 rounded-full transition-all",
                n === i ? "w-6 bg-primary" : "w-1.5 bg-muted hover:bg-muted-foreground/40",
              )}
            />
          ))}
        </div>
        <button
          onClick={() => (last ? onDone() : setI(i + 1))}
          className="rounded-xl bg-primary py-3 text-sm font-semibold text-primary-foreground shadow-lg shadow-primary/20 transition-transform active:scale-[0.99]"
        >
          {last ? "Get started" : "Next"}
        </button>
        {!last && (
          <button
            onClick={() => setI(i + 1)}
            className="-mt-1 text-center text-xs text-muted-foreground"
          >
            {`${i + 1} of ${STEPS.length}`}
          </button>
        )}
      </div>
    </main>
  );
}
