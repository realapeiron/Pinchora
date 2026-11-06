import { Navigate, useSearchParams } from "react-router";

import { Crab } from "../components/Crab";
import { Panel } from "../components/Panel";
import { PixelLink } from "../components/PixelButton";
import { useMe } from "../lib/queries";

const ERRORS: Record<string, string> = {
  denied: "Login was cancelled on Discord. Try again when you are ready.",
  discord: "Discord did not answer just now. Try again in a moment.",
  expired: "Your session ran out. Log in again to carry on.",
};

/** A path on this site to come back to after login. */
function safeNext(next: string | null): string {
  return next && next.startsWith("/") && !next.startsWith("//") && !next.includes("\\") ? next : "/servers";
}

export function Login() {
  const [params] = useSearchParams();
  const next = safeNext(params.get("next"));
  const error = ERRORS[params.get("error") ?? ""];
  const me = useMe();
  if (me.isSuccess) return <Navigate to={next} replace />;
  return (
    <main className="login">
      <div className="login-card">
        <div className="login-brand">
          <Crab scale={8} />
          <h1 className="login-title">PINCHORA</h1>
          <p className="pixel-label muted">SERVER DASHBOARD</p>
        </div>
        <Panel tone="accent">
          <p className="login-lead">
            Set up leveling, welcome cards and more for the servers you run, from your browser.
          </p>
          {error && (
            <p className="notice notice-heart" role="alert">
              {error}
            </p>
          )}
          <PixelLink href={`/auth/login?next=${encodeURIComponent(next)}`} className="login-button">
            Log in with Discord
          </PixelLink>
          <p className="fine-print">
            Pinchora asks Discord for your name and server list only. You need Manage Server in a server to
            change its settings.
          </p>
        </Panel>
      </div>
    </main>
  );
}
