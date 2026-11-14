import { translate as t } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { Field } from "../../../../components/Field";
import { ChannelPicker } from "../../../../components/DashboardPickers";
import { PixelButton } from "../../../../components/PixelButton";
import type { GuildInfo } from "../../../../lib/api";
import type { ModPolicy } from "../../api/moderationDesk";
import type { PolicyChange } from "./DeskBasics";

type Props = {
  draft: Pick<ModPolicy, "autopurge">;
  change: PolicyChange;
  info: GuildInfo | undefined;
};

export function CleanupRules({ draft, change, info }: Props) {
  return (
    <Panel title={t("Automatic cleanup")}>
      <p>
        {draft.autopurge.length}/25 {t("used")}
      </p>
      {draft.autopurge.map((rule, i) => (
        <fieldset key={i}>
          <legend>
            {t("Cleanup rule")} {i + 1}
          </legend>
          <Field label={t("Channel")} htmlFor={`cleanup-channel-${i}`}>
            <ChannelPicker
              id={`cleanup-channel-${i}`}
              info={info}
              value={rule.channel || null}
              onChange={(v) =>
                change(
                  "autopurge",
                  draft.autopurge.map((r, n) => (n === i ? { ...r, channel: v ?? "" } : r)),
                )
              }
            />
          </Field>
          <Field label={t("Age in hours")} htmlFor={`cleanup-age-${i}`}>
            <input
              id={`cleanup-age-${i}`}
              className="pixel-input"
              type="number"
              min={1}
              max={336}
              value={rule.age_secs / 3600}
              onChange={(e) =>
                change(
                  "autopurge",
                  draft.autopurge.map((r, n) =>
                    n === i ? { ...r, age_secs: Number(e.target.value) * 3600 } : r,
                  ),
                )
              }
            />
          </Field>
          <label>
            <input
              type="checkbox"
              checked={rule.bots_only}
              onChange={(e) =>
                change(
                  "autopurge",
                  draft.autopurge.map((r, n) => (n === i ? { ...r, bots_only: e.target.checked } : r)),
                )
              }
            />{" "}
            {t("Bots only")}
          </label>
          <PixelButton
            variant="ghost"
            onClick={() =>
              change(
                "autopurge",
                draft.autopurge.filter((_, n) => n !== i),
              )
            }
          >
            {t("Remove")}
          </PixelButton>
        </fieldset>
      ))}
      <PixelButton
        disabled={draft.autopurge.length >= 25}
        variant="secondary"
        onClick={() =>
          change("autopurge", [...draft.autopurge, { channel: "", age_secs: 86400, bots_only: false }])
        }
      >
        {t("Add cleanup rule")}
      </PixelButton>
    </Panel>
  );
}
