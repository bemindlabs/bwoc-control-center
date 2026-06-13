import { useState } from "react";
import type { ControllerInfo } from "./types";
import { cn } from "./ui";

const STEPS = [
  {
    icon: "🎛️",
    title: "BWOC Control Center",
    body: "Watch and chat with your agent fleet — from anywhere on your private mesh.",
  },
  {
    icon: "🔒",
    title: "Secure by design",
    body: "This device signs every request with its own key. A host operator approves it once — no public ports, everything rides the tailnet.",
  },
  {
    icon: "🛰",
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

  return (
    <main
      className="flex h-[100dvh] flex-col bg-background text-foreground"
      style={{
        paddingTop: "env(safe-area-inset-top)",
        paddingBottom: "calc(env(safe-area-inset-bottom) + 1rem)",
      }}
    >
      <div className="flex justify-end px-5 pt-3">
        {!last && (
          <button
            onClick={onDone}
            className="text-sm text-muted-foreground hover:text-foreground"
          >
            Skip
          </button>
        )}
      </div>

      <div className="flex flex-1 flex-col items-center justify-center gap-5 px-8 text-center">
        <div className="grid size-20 place-items-center rounded-3xl bg-primary/15 text-4xl ring-1 ring-primary/25">
          {step.icon}
        </div>
        <h1 className="text-xl font-semibold">{step.title}</h1>
        <p className="max-w-xs text-sm leading-relaxed text-muted-foreground">
          {step.body}
        </p>
        {last && controller && (
          <p className="rounded-lg border border-border bg-card px-3 py-2 text-xs text-muted-foreground">
            this device · <span className="font-mono">{controller.id}</span>
          </p>
        )}
      </div>

      <div className="flex flex-col gap-4 px-8">
        <div className="flex justify-center gap-1.5">
          {STEPS.map((_, n) => (
            <span
              key={n}
              className={cn(
                "h-1.5 rounded-full transition-all",
                n === i ? "w-5 bg-primary" : "w-1.5 bg-muted",
              )}
            />
          ))}
        </div>
        <button
          onClick={() => (last ? onDone() : setI(i + 1))}
          className="rounded-xl bg-primary py-3 text-sm font-semibold text-primary-foreground"
        >
          {last ? "Get started" : "Next"}
        </button>
      </div>
    </main>
  );
}
