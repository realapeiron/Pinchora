import { translate as t } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { Field } from "../../../../components/Field";
import { ChannelPicker, RolePicker } from "../../../../components/DashboardPickers";
import { PixelButton } from "../../../../components/PixelButton";
import type { GuildInfo } from "../../../../lib/api";
import type { ModPolicy } from "../../api/moderationDesk";
import type { PolicyChange } from "./DeskBasics";

type Props = {
  draft: Pick<ModPolicy, "conditions">;
  change: PolicyChange;
  info: GuildInfo | undefined;
};

export function ConditionalRoles({ draft, change, info }: Props) {
  return (
    <Panel title={t("Conditional roles")}>
      <p>
        {draft.conditions.length}/25 {t("used")}
      </p>
      {draft.conditions.map((rule, i) => (
        <fieldset key={i}>
          <legend>
            {t("Role rule")} {i + 1}
          </legend>
          <Field label={t("Reward role")} htmlFor={`reward-${i}`}>
            <RolePicker
              id={`reward-${i}`}
              info={info}
              value={rule.role || null}
              onChange={(v) =>
                change(
                  "conditions",
                  draft.conditions.map((r, n) => (n === i ? { ...r, role: v ?? "" } : r)),
                )
              }
            />
          </Field>
          <Field label={t("Required role IDs, separated by spaces")} htmlFor={`requires-${i}`}>
            <input
              id={`requires-${i}`}
              className="pixel-input"
              value={rule.requires.join(" ")}
              onChange={(e) =>
                change(
                  "conditions",
                  draft.conditions.map((r, n) =>
                    n === i ? { ...r, requires: e.target.value.split(/\s+/).filter(Boolean) } : r,
                  ),
                )
              }
            />
          </Field>
          <label>
            <input
              type="checkbox"
              checked={rule.all}
              onChange={(e) =>
                change(
                  "conditions",
                  draft.conditions.map((r, n) => (n === i ? { ...r, all: e.target.checked } : r)),
                )
              }
            />{" "}
            {t("Require every role (AND)")}
          </label>
          <Field label={t("While in voice channel")} htmlFor={`voice-${i}`}>
            <ChannelPicker
              id={`voice-${i}`}
              info={info}
              value={rule.voice_channel}
              onChange={(v) =>
                change(
                  "conditions",
                  draft.conditions.map((r, n) => (n === i ? { ...r, voice_channel: v } : r)),
                )
              }
            />
          </Field>
          <PixelButton
            variant="ghost"
            onClick={() =>
              change(
                "conditions",
                draft.conditions.filter((_, n) => n !== i),
              )
            }
          >
            {t("Remove")}
          </PixelButton>
        </fieldset>
      ))}
      <PixelButton
        disabled={draft.conditions.length >= 25}
        variant="secondary"
        onClick={() =>
          change("conditions", [
            ...draft.conditions,
            { role: "", requires: [], all: true, voice_channel: null },
          ])
        }
      >
        {t("Add role rule")}
      </PixelButton>
    </Panel>
  );
}
