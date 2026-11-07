import { useState } from "react";
import { useOutletContext } from "react-router";
import { translate as t } from "../i18n";
import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { RolePicker, ChannelPicker } from "../components/Pickers";
import { SaveBar } from "../components/SaveBar";
import { useDiscord } from "../lib/queries";
import { type Overview } from "../lib/api";
import {
  lookupRanks,
  nicknamePreview,
  type NewBind,
  type RobloxData,
  type RobloxSettings,
  useRoblox,
  useRobloxWrite,
} from "../lib/roblox";
import { Loading, Problem } from "./States";
import { PageHead } from "./PageHead";
import "../styles/roblox.css";

export function Roblox() {
  const overview = useOutletContext<Overview>();
  const guild = overview.guild.id;
  const query = useRoblox(guild);
  return (
    <div className="settings-page roblox-page">
      <PageHead
        title={t("Roblox verification")}
        lead={t("Link Roblox profiles, then assign safe roles from group membership and ranks.")}
      />
      {query.isPending && <Loading />}
      {query.isError && <Problem error={query.error} />}
      {query.data && (
        <RobloxEditor key={JSON.stringify(query.data.settings)} guild={guild} data={query.data} />
      )}
    </div>
  );
}
function RobloxEditor({ guild, data }: { guild: string; data: RobloxData }) {
  const discord = useDiscord(guild).data;
  const { save, add, remove } = useRobloxWrite(guild);
  const [draft, setDraft] = useState<RobloxSettings>(data.settings);
  const [kind, setKind] = useState<NewBind["kind"]>("verified");
  const [role, setRole] = useState<string | null>(null);
  const [group, setGroup] = useState("");
  const [minimum, setMinimum] = useState(1);
  const [maximum, setMaximum] = useState(255);
  const [removeUnmet, setRemoveUnmet] = useState(true);
  const [ranks, setRanks] = useState<{ rank: number; name: string }[]>([]);
  const [rankError, setRankError] = useState("");
  const [notice, setNotice] = useState("");
  const change = <K extends keyof RobloxSettings>(key: K, value: RobloxSettings[K]) =>
    setDraft((old) => ({ ...old, [key]: value }));
  const dirty = JSON.stringify(draft) !== JSON.stringify(data.settings);
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
  const preview = nicknamePreview(
    draft.nickname_template ?? "{roblox-name}",
    "DiscordMember",
    "RobloxPlayer",
  );
  return (
    <>
      <Panel title={t("Verification settings")}>
        <SwitchRow
          title={t("Enable Roblox verification")}
          control={
            <PixelToggle
              label={t("Enable Roblox verification")}
              checked={draft.enabled}
              onChange={(v) => change("enabled", v)}
            />
          }
        >
          <span>{t("Members use /roblox verify to link their profile.")}</span>
        </SwitchRow>
        <SwitchRow
          title={t("Hold unverified newcomers")}
          control={
            <PixelToggle
              label={t("Hold unverified newcomers")}
              checked={draft.require_verified}
              onChange={(v) => change("require_verified", v)}
            />
          }
        >
          <span>{t("Requires the server join gate and a verification channel.")}</span>
        </SwitchRow>
        <div className="settings-grid">
          <Field label={t("Verified role")} htmlFor="rb-verified">
            <RolePicker
              id="rb-verified"
              info={discord}
              value={draft.verified_role_id}
              onChange={(v) => change("verified_role_id", v)}
              assignable
              forActions
              none={t("No role")}
            />
          </Field>
          <Field label={t("Unverified role")} htmlFor="rb-unverified">
            <RolePicker
              id="rb-unverified"
              info={discord}
              value={draft.unverified_role_id}
              onChange={(v) => change("unverified_role_id", v)}
              assignable
              forActions
              none={t("No role")}
            />
          </Field>
          <Field label={t("Verification channel")} htmlFor="rb-channel">
            <ChannelPicker
              id="rb-channel"
              info={discord}
              value={draft.verify_channel_id}
              onChange={(v) => change("verify_channel_id", v)}
              none={t("No channel")}
            />
          </Field>
          <Field
            label={t("Nickname template")}
            htmlFor="rb-template"
            hint={t("Use {roblox-name} and {discord-name}. Leave empty to keep Discord names.")}
          >
            <input
              id="rb-template"
              className="pixel-input"
              value={draft.nickname_template ?? ""}
              onChange={(e) => change("nickname_template", e.target.value || null)}
              maxLength={128}
            />
          </Field>
        </div>
        <p className="muted">
          {t("Preview")}: <strong>{preview}</strong>
        </p>
      </Panel>
      <Panel title={t("Role binds")}>
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
      </Panel>
      {notice && <p role="status">{notice}</p>}
      <SaveBar
        count={dirty ? 1 : 0}
        saving={save.isPending}
        error={save.error?.message}
        onSave={() => save.mutate(draft, { onSuccess: () => setNotice(t("Roblox settings saved.")) })}
        onReset={() => setDraft(data.settings)}
      />
    </>
  );
}
