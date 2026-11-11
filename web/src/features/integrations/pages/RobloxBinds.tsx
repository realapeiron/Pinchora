import { useState } from "react";
import { translate as t } from "../../../i18n";
import { Field, SwitchRow } from "../../../components/Field";
import { PixelButton } from "../../../components/PixelButton";
import { PixelToggle } from "../../../components/PixelToggle";
import { RolePicker } from "../../../components/Pickers";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { robloxSummary } from "./robloxSummary";
import { lookupRanks, type NewBind, type RobloxData, useRobloxWrite } from "../api/roblox";
import type { GuildInfo } from "../../../lib/api";

export function RobloxBinds({
  guild,
  data,
  discord,
}: {
  guild: string;
  data: RobloxData;
  discord: GuildInfo | undefined;
}) {
  const { add, remove } = useRobloxWrite(guild);
  const [kind, setKind] = useState<NewBind["kind"]>("verified");
  const [role, setRole] = useState<string | null>(null);
  const [group, setGroup] = useState("");
  const [minimum, setMinimum] = useState(1);
  const [maximum, setMaximum] = useState(255);
  const [removeUnmet, setRemoveUnmet] = useState(true);
  const [ranks, setRanks] = useState<{ rank: number; name: string }[]>([]);
  const [rankError, setRankError] = useState("");
  const [notice, setNotice] = useState("");
  const search = async () => {
    setRankError("");
    try {
      const result = await lookupRanks(guild, group);
      setRanks(result.roles ?? []);
    } catch (error) {
      setRanks([]);
      setRankError(error instanceof Error ? error.message : t("Group lookup failed."));
    }
  };
  const addBind = () => {
    if (!role) return;
    const bind: NewBind = {
      kind,
      role_id: role,
      group_id: kind === "verified" ? null : group,
      rank_min: kind === "group_rank" ? minimum : null,
      rank_max: kind === "group_rank" ? maximum : null,
      remove_if_unmet: removeUnmet,
    };
    add.mutate(bind, {
      onSuccess: () => {
        setNotice(t("Role bind added."));
        setRole(null);
      },
      onError: (e) => setNotice(e.message),
    });
  };
  return (
    <SettingsRow
      title={t("Role binds")}
      description={dt("Assign roles from Roblox conditions.")}
      summary={robloxSummary(data.settings, data.binds, discord).binds}
    >
      <p className="muted">
        {t("A bind gives a role only while the Roblox condition matches. Up to 25 binds per server.")}
      </p>
      {data.binds.length === 0 ? (
        <p>{t("No role binds yet.")}</p>
      ) : (
        <ul className="roblox-bind-list">
          {data.binds.map((b) => (
            <li key={b.id}>
              <span>
                #{b.id} · {b.kind.replace("_", " ")} {b.group_id && `· ${b.group_id}`}{" "}
                {b.rank_min && `· ${b.rank_min}–${b.rank_max}`} ·{" "}
                {discord?.roles.find((r) => r.id === b.role_id)?.name ?? b.role_id}
              </span>
              <PixelButton
                variant="secondary"
                onClick={() => remove.mutate(b.id, { onError: (e) => setNotice(e.message) })}
              >
                {t("Remove")}
              </PixelButton>
            </li>
          ))}
        </ul>
      )}
      <div className="settings-grid">
        <Field label={t("Condition")} htmlFor="rb-kind">
          <select
            id="rb-kind"
            className="pixel-input pixel-select"
            value={kind}
            onChange={(e) => setKind(e.target.value as NewBind["kind"])}
          >
            <option value="verified">{t("Verified account")}</option>
            <option value="group_member">{t("Group member")}</option>
            <option value="group_rank">{t("Group rank")}</option>
          </select>
        </Field>
        <Field label={t("Role to grant")} htmlFor="rb-role">
          <RolePicker
            id="rb-role"
            info={discord}
            value={role}
            onChange={setRole}
            assignable
            forActions
            none={t("Choose a role")}
          />
        </Field>
      </div>
      {kind !== "verified" && (
        <Field label={t("Roblox group ID")} htmlFor="rb-group">
          <div className="roblox-inline">
            <input
              id="rb-group"
              className="pixel-input"
              type="number"
              min="1"
              value={group}
              onChange={(e) => setGroup(e.target.value)}
            />
            <PixelButton variant="secondary" onClick={search} disabled={!group}>
              {t("Find ranks")}
            </PixelButton>
          </div>
        </Field>
      )}
      {rankError && <p role="alert">{rankError}</p>}
      {ranks.length > 0 && <p className="muted">{ranks.map((r) => `${r.rank} ${r.name}`).join(" · ")}</p>}
      {kind === "group_rank" && (
        <div className="settings-grid">
          <Field label={t("Minimum rank")} htmlFor="rb-min">
            <input
              id="rb-min"
              className="pixel-input"
              type="number"
              min="1"
              max="255"
              value={minimum}
              onChange={(e) => setMinimum(Number(e.target.value))}
            />
          </Field>
          <Field label={t("Maximum rank")} htmlFor="rb-max">
            <input
              id="rb-max"
              className="pixel-input"
              type="number"
              min="1"
              max="255"
              value={maximum}
              onChange={(e) => setMaximum(Number(e.target.value))}
            />
          </Field>
        </div>
      )}
      <SwitchRow
        title={t("Remove role when unmatched")}
        control={
          <PixelToggle
            label={t("Remove role when unmatched")}
            checked={removeUnmet}
            onChange={setRemoveUnmet}
          />
        }
      />
      <PixelButton
        onClick={addBind}
        disabled={!role || (kind !== "verified" && !group) || add.isPending || data.binds.length >= 25}
      >
        {t("Add bind")}
      </PixelButton>
      {notice && <p role="status">{notice}</p>}
    </SettingsRow>
  );
}
