import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../../i18n";
import { Field, SwitchRow } from "../../../../components/Field";
import { Panel } from "../../../../components/Panel";
import { PixelToggle } from "../../../../components/PixelToggle";
import type { CommandDef } from "../../../../lib/api";
import { Choice } from "../../../../components/Choice";
import { LIMITS } from "../../../../lib/tags";
import type { TagEditor } from "../../hooks/useTagEditor";

type KEYS = "name" | "description" | "reply_mode" | "enabled";
type Props = {
  draft: Pick<CommandDef, KEYS>;
  set: TagEditor<CommandDef>["set"];
  problem: TagEditor<CommandDef>["problem"];
  nameProblem: string | null;
};

export function CommandBasics({ draft, set, problem, nameProblem }: Props) {
  return (
    <Panel title={deferredText("BASICS")}>
      <div className="form-grid form-grid-2">
        <Field
          label={spaTranslate("Name")}
          htmlFor="command-name"
          error={nameProblem}
          hint={deferredText("What members type after /tag. Lowercase letters, numbers, - and _.")}
        >
          <div className="input-prefix">
            <span className="input-prefix-text" aria-hidden="true">
              /tag
            </span>
            <input
              id="command-name"
              className="pixel-input"
              value={draft.name}
              maxLength={LIMITS.name_max + 1}
              autoComplete="off"
              onChange={(event) => set("name", event.target.value)}
              aria-invalid={nameProblem ? true : undefined}
            />
          </div>
        </Field>
        <Field
          label={spaTranslate("Description")}
          htmlFor="command-description"
          error={problem("description")}
          hint={deferredText("Shown next to the name in suggestions. Optional.")}
          aside={
            <span className="counter">
              {[...draft.description].length}/{LIMITS.description_max}
            </span>
          }
        >
          <input
            id="command-description"
            className="pixel-input"
            value={draft.description}
            maxLength={LIMITS.description_max}
            onChange={(event) => set("description", event.target.value)}
          />
        </Field>
      </div>
      <Choice
        name="Who sees the answer"
        value={draft.reply_mode}
        options={[
          {
            value: "public",
            title: deferredText("Everyone in the channel"),
            detail: deferredText("The answer is a normal message."),
          },
          {
            value: "ephemeral",
            title: deferredText("Only the person who ran it"),
            detail: deferredText("A private answer."),
          },
        ]}
        onChange={(next) => set("reply_mode", next)}
      />
      <SwitchRow
        title={deferredText("Switched on")}
        control={
          <PixelToggle
            label={deferredText("Switched on")}
            checked={draft.enabled}
            onChange={(next) => set("enabled", next)}
          />
        }
      >
        {" "}
        {deferredText("Off, members are told it is switched off and it leaves the suggestions.")}{" "}
      </SwitchRow>
    </Panel>
  );
}
