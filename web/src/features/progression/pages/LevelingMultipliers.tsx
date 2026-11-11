import { translate as spaTranslate } from "../../../i18n";
import { useState } from "react";
import { Field } from "../../../components/Field";
import { ChannelPicker, ChannelTag, RolePicker, RoleTag } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { RichText } from "../../../components/RichText";
import { SettingsRow } from "../../../components/SettingsRow";
import { t } from "../../../i18n/dashboard";
import { levelingSummary } from "./levelingSummary";
import type { GuildInfo } from "../../../lib/api";
import { LIMITS, checkMultiplier, problem } from "../../../lib/rules";
import { useXpRules } from "../../../lib/queries";

/** The message of a failed list write, for the add form. */
function writeError(error: unknown): string | null {
  return error instanceof Error ? error.message : null;
}

export function Multipliers({ guild, info }: { guild: string; info: GuildInfo | undefined }) {
  const { query, write } = useXpRules(guild);
  const [kind, setKind] = useState<"channel" | "role">("channel");
  const [target, setTarget] = useState<string | null>(null);
  const [multiplier, setMultiplier] = useState("2");
  const [confirmClear, setConfirmClear] = useState(false);
  const local =
    multiplier === "" ? spaTranslate("Write a multiplier.") : problem(checkMultiplier(Number(multiplier)));
  const items = query.data?.items ?? [];
  const add = (value: number) =>
    target &&
    write.mutate(
      { method: "PUT", sub: `/${kind}/${target}`, body: { multiplier: value } },
      { onSuccess: () => setTarget(null) },
    );

  return (
    <SettingsRow
      rowId="leveling-multipliers-xp-multipliers"
      title={t("row.XP multipliers", {}, "XP MULTIPLIERS")}
      description={t("Scale XP for channels and roles.")}
      summary={levelingSummary({}, { multipliers: items.length }).multipliers}
    >
      <div className="settings-detail-fields">
        <span className="pixel-label muted">
          {t("{count} of {max} XP multipliers used", {
            count: items.length,
            max: query.data?.max ?? LIMITS.max_xp_rules,
          })}
        </span>
        <p className="muted panel-lead">
          {" "}
          {spaTranslate(
            "Scale the XP and points a channel or role earns: 2 doubles it, 0 earns none. A channel's multiplier times the member's best role multiplier; a no-XP role always wins. Server XP only; global XP stays normal.",
          )}{" "}
        </p>
        {items.length === 0 ? (
          <p className="empty-row muted">{spaTranslate("No multipliers yet: everything earns normal XP.")}</p>
        ) : (
          <ul className="list" aria-label={spaTranslate("XP multipliers")}>
            {items.map((rule) => (
              <li key={`${rule.kind}:${rule.target_id}`} className="list-row">
                <span className="list-main">
                  {rule.kind === "channel" ? (
                    <ChannelTag info={info} id={rule.target_id} />
                  ) : (
                    <RoleTag info={info} id={rule.target_id} />
                  )}
                </span>
                <span className={`badge ${rule.multiplier === 0 ? "badge-heart" : "badge-gold"}`}>
                  {rule.multiplier === 0 ? spaTranslate("NO XP") : `×${rule.multiplier}`}
                </span>
                <PixelButton
                  variant="ghost"
                  className="button-small"
                  aria-label={spaTranslate("Remove multiplier")}
                  disabled={write.isPending}
                  onClick={() => write.mutate({ method: "DELETE", sub: `/${rule.kind}/${rule.target_id}` })}
                >
                  {spaTranslate("Remove")}
                </PixelButton>
              </li>
            ))}
          </ul>
        )}
        <div className="add-row add-row-rule">
          <Field label={spaTranslate("For")} htmlFor="rule-kind">
            <select
              id="rule-kind"
              className="pixel-input pixel-select"
              value={kind}
              onChange={(event) => {
                setKind(event.target.value as "channel" | "role");
                setTarget(null);
              }}
            >
              <option value="channel">{spaTranslate("A channel or category")}</option>
              <option value="role">{spaTranslate("A role")}</option>
            </select>
          </Field>
          <Field
            label={kind === "channel" ? spaTranslate("Channel") : spaTranslate("Role")}
            htmlFor="rule-target"
          >
            {kind === "channel" ? (
              <ChannelPicker id="rule-target" info={info} value={target} onChange={setTarget} any />
            ) : (
              <RolePicker id="rule-target" info={info} value={target} onChange={setTarget} everyone />
            )}
          </Field>
          <Field label={spaTranslate("Multiplier")} htmlFor="rule-value" error={local}>
            <input
              id="rule-value"
              className="pixel-input input-short"
              type="number"
              step="0.25"
              min={0}
              max={LIMITS.multiplier[1]}
              value={multiplier}
              onChange={(event) => setMultiplier(event.target.value)}
            />
          </Field>
          <div className="add-actions">
            <PixelButton
              disabled={!target || !!local || write.isPending}
              onClick={() => add(Number(multiplier))}
            >
              {" "}
              {spaTranslate("Set")}{" "}
            </PixelButton>
            <PixelButton variant="secondary" disabled={!target || write.isPending} onClick={() => add(0)}>
              {" "}
              {spaTranslate("No XP")}{" "}
            </PixelButton>
          </div>
        </div>
        {write.isError && (
          <p className="field-error" role="alert">
            <RichText text={writeError(write.error) ?? ""} />
          </p>
        )}
        {items.length > 0 && (
          <div className="panel-foot">
            {confirmClear ? (
              <>
                <span>
                  {spaTranslate("Remove all")} {items.length} {spaTranslate("multipliers?")}
                </span>
                <PixelButton variant="ghost" onClick={() => setConfirmClear(false)}>
                  {" "}
                  {spaTranslate("Keep them")}{" "}
                </PixelButton>
                <PixelButton
                  onClick={() =>
                    write.mutate({ method: "DELETE", sub: "" }, { onSettled: () => setConfirmClear(false) })
                  }
                >
                  {" "}
                  {spaTranslate("Remove all")}{" "}
                </PixelButton>
              </>
            ) : (
              <PixelButton variant="ghost" onClick={() => setConfirmClear(true)}>
                {spaTranslate("Clear all")}
              </PixelButton>
            )}
          </div>
        )}
      </div>
    </SettingsRow>
  );
}
