import { Crab } from "../components/Crab";
import { Panel } from "../components/Panel";
import { PixelLink } from "../components/PixelButton";

export function NotFound() {
  return (
    <main className="state state-full">
      <Crab scale={6} />
      <Panel title="LOST AT SEA">
        <p>There is no page here.</p>
        <PixelLink href="/servers">Back to your servers</PixelLink>
      </Panel>
    </main>
  );
}
