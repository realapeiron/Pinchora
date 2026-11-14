import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../../i18n";
import { IdList } from "../../components/IdList";
import { Field } from "../../../../components/Field";
import { Panel } from "../../../../components/Panel";
import type { CommandDef, GuildInfo } from "../../../../lib/api";
import { LIMITS } from "../../../../lib/tags";
import type { TagEditor } from "../../hooks/useTagEditor";

type KEYS = "allow_roles" | "deny_roles" | "allow_channels" | "deny_channels" | "cooldown_secs" | "actions";
type Props = {
  draft: Pick<CommandDef, KEYS>;
  set: TagEditor<CommandDef>["set"];
  problem: TagEditor<CommandDef>["problem"];
  info: GuildInfo | undefined;
};

export function CommandAccess({ draft, set, problem, info }: Props) {
  return (
    <Panel title={deferredText("WHO CAN USE IT, AND WHERE")}>
      <div className="form-grid form-grid-2">
        <Field
          label={deferredText("Only these roles")}
          error={problem("allow_roles")}
          hint={deferredText("Empty: everyone may.")}
        >
          <IdList
            kind="role"
            info={info}
            ids={draft.allow_roles}
            onChange={(next) => set("allow_roles", next)}
            max={LIMITS.max_list}
            name="Allowed roles"
            none={deferredText("Every role.")}
          />
        </Field>
        <Field
          label={deferredText("Never these roles")}
          error={problem("deny_roles")}
          hint={deferredText("A denied role wins.")}
        >
          <IdList
            kind="role"
            info={info}
            ids={draft.deny_roles}
            onChange={(next) => set("deny_roles", next)}
            max={LIMITS.max_list}
            name="Denied roles"
            none={deferredText("No role is turned away.")}
          />
        </Field>
        <Field
          label={deferredText("Only in these channels")}
          error={problem("allow_channels")}
          hint={deferredText("A category covers the channels under it. Empty: everywhere.")}
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
      <Field
        label={spaTranslate("Cooldown")}
        htmlFor="command-cooldown"
        error={problem("cooldown_secs")}
        hint={deferredText(
          "Seconds a member waits between uses, up to {arg1}. 0 is none. With actions, at least {arg2}.",
          { arg1: LIMITS.command_cooldown_max, arg2: LIMITS.action_cooldown_min },
        )}
      >
        <input
          id="command-cooldown"
          type="number"
          className="pixel-input input-short"
          min={draft.actions.length > 0 ? LIMITS.action_cooldown_min : 0}
          max={LIMITS.command_cooldown_max}
          value={draft.cooldown_secs}
          onChange={(event) => set("cooldown_secs", Number(event.target.value))}
        />
      </Field>
    </Panel>
  );
}
