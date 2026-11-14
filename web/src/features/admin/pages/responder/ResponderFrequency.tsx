import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../../i18n";
import { Field } from "../../../../components/Field";
import { Panel } from "../../../../components/Panel";
import type { GuildInfo, ResponderDef } from "../../../../lib/api";
import { IdList } from "../../components/IdList";
import { LIMITS } from "../../../../lib/tags";
import type { TagEditor } from "../../hooks/useTagEditor";

type Props = {
  draft: Pick<
    ResponderDef,
    "trigger_kind" | "chance" | "cooldown_secs" | "actions" | "allow_channels" | "deny_channels"
  >;
  set: TagEditor<ResponderDef>["set"];
  problem: TagEditor<ResponderDef>["problem"];
  info: GuildInfo | undefined;
};

export function ResponderFrequency({ draft, set, problem, info }: Props) {
  const hearsChat = draft.trigger_kind === "keyword" || draft.trigger_kind === "level_up";
  return (
    <Panel title={deferredText("HOW OFTEN")}>
      <div className="form-grid form-grid-2">
        <Field
          label={spaTranslate("Chance")}
          htmlFor="responder-chance"
          error={problem("chance")}
          hint={deferredText("Percent of the times it fires that it answers.")}
        >
          <input
            id="responder-chance"
            type="number"
            className="pixel-input input-short"
            min={1}
            max={100}
            value={draft.chance}
            onChange={(event) => set("chance", Number(event.target.value))}
          />
        </Field>
        <Field
          label={spaTranslate("Cooldown")}
          htmlFor="responder-cooldown"
          error={problem("cooldown_secs")}
          hint={deferredText(
            "Seconds before it can answer the same member again, up to {arg1}. With actions, at least {arg2}.",
            { arg1: LIMITS.responder_cooldown_max, arg2: LIMITS.action_cooldown_min },
          )}
        >
          <input
            id="responder-cooldown"
            type="number"
            className="pixel-input input-short"
            min={draft.actions.length > 0 ? LIMITS.action_cooldown_min : 0}
            max={LIMITS.responder_cooldown_max}
            value={draft.cooldown_secs}
            onChange={(event) => set("cooldown_secs", Number(event.target.value))}
          />
        </Field>
      </div>
      {hearsChat && (
        <div className="form-grid form-grid-2">
          <Field
            label={deferredText("Only in these channels")}
            error={problem("allow_channels")}
            hint={deferredText(
              "Where the message was sent, or the member levelled up. A category covers the channels under it.",
            )}
          >
            <IdList
              kind="channel"
              info={info}
              ids={draft.allow_channels}
              onChange={(next) => set("allow_channels", next)}
              max={LIMITS.max_list}
              name="Allowed channels"
              none={deferredText("Every channel.")}
            />
          </Field>
          <Field label={deferredText("Never in these channels")} error={problem("deny_channels")}>
            <IdList
              kind="channel"
              info={info}
              ids={draft.deny_channels}
              onChange={(next) => set("deny_channels", next)}
              max={LIMITS.max_list}
              name="Denied channels"
              none={deferredText("No channel is off limits.")}
            />
          </Field>
        </div>
      )}
    </Panel>
  );
}
