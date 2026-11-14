import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import type { ResponderDef } from "../../../../lib/api";
import type { TagEditor } from "../../hooks/useTagEditor";
import { Field, SwitchRow } from "../../../../components/Field";
import { PixelToggle } from "../../../../components/PixelToggle";
import { LIMITS } from "../../../../lib/tags";

type Props = {
  draft: Pick<ResponderDef, "name" | "enabled" | "trigger_kind">;
  set: TagEditor<ResponderDef>["set"];
  problem: TagEditor<ResponderDef>["problem"];
  keywordLocked: boolean;
};

export function ResponderBasics({ draft, set, problem, keywordLocked }: Props) {
  const kind = draft.trigger_kind;
  return (
    <Panel title={deferredText("BASICS")}>
      <Field
        label={spaTranslate("Name")}
        htmlFor="responder-name"
        error={problem("name")}
        hint={deferredText("Only for you, to tell responders apart.")}
      >
        <input
          id="responder-name"
          className="pixel-input"
          value={draft.name}
          maxLength={LIMITS.responder_name_max}
          autoComplete="off"
          onChange={(event) => set("name", event.target.value)}
        />
      </Field>
      <SwitchRow
        title={deferredText("Switched on")}
        control={
          <PixelToggle
            label={deferredText("Switched on")}
            checked={draft.enabled}
            disabled={kind === "keyword" && keywordLocked && !draft.enabled}
            onChange={(next) => set("enabled", next)}
          />
        }
      >
        {kind === "keyword" && keywordLocked
          ? deferredText("Keyword triggers can't be switched on while the bot can't read chat.")
          : deferredText("Off, it stays written but the bot never sends it.")}
      </SwitchRow>
    </Panel>
  );
}
