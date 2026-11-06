import { ApiError } from "../lib/api";
import { Crab } from "../components/Crab";
import { Panel } from "../components/Panel";

export function Loading({ full, label = "Loading" }: { full?: boolean; label?: string }) {
  return (
    <div className={full ? "state state-full" : "state"} role="status">
      <Crab scale={4} className="crab-walk" />
      <span className="pixel-label">{label}…</span>
    </div>
  );
}

export function Problem({ error, full }: { error: unknown; full?: boolean }) {
  const message =
    error instanceof ApiError ? error.message : "Something went wrong on our side. Please try again.";
  return (
    <div className={full ? "state state-full" : "state"}>
      <Panel title="Hmm" tone="accent">
        <p>{message}</p>
      </Panel>
    </div>
  );
}
