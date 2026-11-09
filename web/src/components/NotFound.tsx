import { translate as spaTranslate } from "../i18n";
import { Crab } from "./Crab";
import { Panel } from "./Panel";
import { PixelLink } from "./PixelButton";

export function NotFound() {
  return (
    <main className="state state-full">
      <Crab scale={6} />
      <Panel title={spaTranslate("LOST AT SEA")}>
        <p>{spaTranslate("There is no page here.")}</p>
        <PixelLink href="/servers">{spaTranslate("Back to your servers")}</PixelLink>
      </Panel>
    </main>
  );
}
