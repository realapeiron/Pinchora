import { t as deferredText } from "../../../../i18n/dashboard";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { type StudioModel } from "../../hooks/useMessageStudio";

export function StarterTemplates({ useStarter }: Pick<StudioModel, "useStarter">) {
  return (
    <Panel title={deferredText("STARTER TEMPLATES")}>
      <p className="muted">{deferredText("Choose a starting point, then customize and save it as a kit.")}</p>
      <div className="studio-actions">
        <PixelButton variant="secondary" onClick={() => useStarter("welcome")}>
          {deferredText("Welcome kit")}
        </PixelButton>
        <PixelButton variant="secondary" onClick={() => useStarter("announcement")}>
          {deferredText("Announcement kit")}
        </PixelButton>
        <PixelButton variant="secondary" onClick={() => useStarter("faq")}>
          {deferredText("FAQ kit")}
        </PixelButton>
      </div>
    </Panel>
  );
}
