import { useEffect, useState } from "react";
import { HubMark } from "./Home";

/** Launch screen: the BWOC dharmachakra mark on the calm slate field, held for
 *  a beat before the app reveals onboarding (or the fleet). Fades itself out in
 *  the final ~400ms so the handoff feels like one motion, not a cut. */
export default function Splash({
  durationMs = 3000,
  onDone,
}: {
  durationMs?: number;
  onDone: () => void;
}) {
  const [leaving, setLeaving] = useState(false);

  useEffect(() => {
    const fade = setTimeout(() => setLeaving(true), durationMs - 400);
    const done = setTimeout(onDone, durationMs);
    return () => {
      clearTimeout(fade);
      clearTimeout(done);
    };
  }, [durationMs, onDone]);

  return (
    <main
      className={`grid h-[100dvh] place-items-center bg-background text-primary transition-opacity duration-[400ms] ${
        leaving ? "opacity-0" : "opacity-100"
      }`}
    >
      <div className="relative flex flex-col items-center gap-6">
        {/* teal glow */}
        <div
          aria-hidden
          className="pointer-events-none absolute left-1/2 top-1/2 -z-10 size-64 -translate-x-1/2 -translate-y-[60%] rounded-full bg-primary/25 blur-[72px]"
        />
        <HubMark className="size-28 animate-[spin_24s_linear_infinite] motion-reduce:animate-none" />
        <div className="flex flex-col items-center gap-1 text-center">
          <h1 className="bg-gradient-to-b from-foreground to-foreground/70 bg-clip-text text-xl font-bold text-transparent">
            BWOC Control Center
          </h1>
          <p className="text-xs tracking-wide text-muted-foreground">
            Buddhist Way of Coding
          </p>
        </div>
        {/* loading dots */}
        <div className="mt-2 flex gap-1.5">
          {[0, 1, 2].map((i) => (
            <span
              key={i}
              className="size-1.5 animate-pulse rounded-full bg-primary/70"
              style={{ animationDelay: `${i * 0.18}s` }}
            />
          ))}
        </div>
      </div>
    </main>
  );
}
