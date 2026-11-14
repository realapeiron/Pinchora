import { translate as t } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { Field } from "../../../../components/Field";
import { ChannelPicker, RolePicker } from "../../../../components/DashboardPickers";
import type { GuildInfo } from "../../../../lib/api";
import type { ModPolicy } from "../../api/moderationDesk";

export type PolicyChange = <K extends keyof ModPolicy>(key: K, value: ModPolicy[K]) => void;

type Props = {
  draft: Pick<
    ModPolicy,
    "report_channel" | "appeals" | "booster_personal" | "mute_role" | "jail_role" | "booster_role"
  >;
  change: PolicyChange;
  info: GuildInfo | undefined;
};

export function DeskBasics({ draft, change, info }: Props) {
  return (
    <>
      <Panel title={t("Staff inbox")}>
        <Field label={t("Report channel")} htmlFor="report-inbox">
          <ChannelPicker
            posting
            id="report-inbox"
            info={info}
            value={draft.report_channel}
            onChange={(v) => change("report_channel", v)}
          />
        </Field>
        <label>
          <input
            type="checkbox"
            checked={draft.appeals}
            onChange={(e) => change("appeals", e.target.checked)}
          />{" "}
          {t("Allow ban and warn appeals")}
        </label>
        <p className="muted">
          {t(
            "Members appeal at /appeals/server-id/case-number after signing in. Only the affected member can submit.",
          )}
        </p>
      </Panel>
      <Panel title={t("Restriction and reward roles")}>
        <label>
          <input
            type="checkbox"
            checked={draft.booster_personal}
            onChange={(e) => change("booster_personal", e.target.checked)}
          />
          {t("Personal booster roles")}
        </label>
        {(
          [
            ["mute_role", "Mute role"],
            ["jail_role", "Jail role"],
            ["booster_role", "Booster reward role"],
          ] as const
        ).map(([key, label]) => (
          <Field key={key} label={t(label)} htmlFor={key}>
            <RolePicker id={key} info={info} value={draft[key]} onChange={(v) => change(key, v)} />
          </Field>
        ))}
        <p className="muted">
          {t("Saved roles are restored on release only when they remain safe and below staff and bot roles.")}
        </p>
      </Panel>
    </>
  );
}
