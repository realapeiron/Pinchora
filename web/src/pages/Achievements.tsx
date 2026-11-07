import { useCallback, useState } from "react";
import { useOutletContext } from "react-router";

import { Field } from "../components/Field";
import { Panel } from "../components/Panel";
import { ChannelPicker, RolePicker } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { Toast } from "../components/Toast";
import { AtlasThumb } from "../components/card/CardProperties";
import {
  useAchievements,
  useAchievementHolders,
  type Achievement,
  type AchievementDraft,
} from "../lib/achievements";
import { ApiError, type Overview } from "../lib/api";
import { useAtlasIndex, useDiscord } from "../lib/queries";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";
import "../styles/achievements.css";

const SYMBOLS: Record<string, string> = {
  star: "★",
  shell: "◉",
  fish: "◆",
  crab: "✦",
  crown: "♛",
  heart: "♥",
  spark: "✧",
  trophy: "★",
  chat: "▣",
  clock: "◷",
  ticket: "▤",
  gem: "♦",
};
const KIND_LABELS: Record<string, string> = {
  messages: "Messages",
  voice_minutes: "Voice minutes",
  level: "Level",
  days_active: "Days active",
  invites: "Invites",
  stars_received: "Stars received",
  tickets_closed: "Tickets closed",
  reputation: "Reputation",
  games_won: "Games won",
  fish_caught: "Fish caught",
  custom: "Custom",
};
const KIND_MODULES: Record<string, string> = {
  messages: "stats",
  voice_minutes: "stats",
  days_active: "stats",
  level: "leveling",
  invites: "invites",
  stars_received: "starboard",
  tickets_closed: "tickets",
  reputation: "economy",
  games_won: "profile",
  fish_caught: "fishing",
};
const fresh = (): AchievementDraft => ({
  name: "",
  description: "",
  icon: "star",
  kind: "messages",
  target: 10,
  reward_role_id: null,
  reward_points: 0,
  announce_channel_id: null,
  secret: false,
  enabled: true,
});

export function Achievements() {
  const overview = useOutletContext<Overview>();
  const guild = overview.guild.id;
  const { query, save, remove } = useAchievements(guild);
  const discord = useDiscord(guild);
  const atlas = useAtlasIndex();
  const [editing, setEditing] = useState<string | "new" | null>(null);
  const [draft, setDraft] = useState<AchievementDraft>(fresh);
  const [preview, setPreview] = useState(false);
  const [holders, setHolders] = useState<string | null>(null);
  const holdersQuery = useAchievementHolders(guild, holders);
  const [toast, setToast] = useState<string | null>(null);
  const closeToast = useCallback(() => setToast(null), []);

  if (query.isPending) return <Loading />;
  if (query.isError) return <Problem error={query.error} />;
  const data = query.data;
  const t = (key: string) => data.strings[key] ?? key;
  const open = (row?: Achievement) => {
    setEditing(row?.id ?? "new");
    setDraft(
      row
        ? {
            name: row.name,
            description: row.description,
            icon: row.icon,
            kind: row.kind,
            target: row.target,
            reward_role_id: row.reward_role_id,
            reward_points: row.reward_points,
            announce_channel_id: row.announce_channel_id,
            secret: row.secret,
            enabled: row.enabled,
          }
        : fresh(),
    );
    setPreview(false);
    save.reset();
  };
  const submit = () =>
    save.mutate(
      { id: editing === "new" ? undefined : (editing ?? undefined), draft },
      {
        onSuccess: () => {
          setEditing(null);
          setToast(t("Saved."));
        },
      },
    );
  const removeItem = (id: string) =>
    remove.mutate(id, {
      onSuccess: () => {
        setEditing(null);
        setHolders(null);
        setToast(t("Removed."));
      },
    });
  const error = save.error instanceof ApiError ? save.error : null;
  const fieldError = (field: string) => (error?.field === field ? error.message : null);
  const off =
    !!KIND_MODULES[draft.kind] &&
    overview.modules.some((m) => m.key === KIND_MODULES[draft.kind] && !m.enabled);
  const badgeIcons = Object.keys(atlas.data ?? {})
    .filter((id) => id.startsWith("b_"))
    .slice(0, 80);
  return (
    <div className="settings-page ach-page">
      <PageHead
        title={t("ACHIEVEMENTS")}
        lead={t("Create milestones that celebrate what members do in your server.")}
        action={
          <PixelButton onClick={() => open()} disabled={data.items.length >= data.max}>
            {t("Add achievement")}
          </PixelButton>
        }
      />
      <div className="ach-summary">
        <span className="pixel-label">{t("Achievements")}</span>
        <strong>
          {data.items.length}
          <span>/{data.max}</span>
        </strong>
        <div className="ach-summary-track">
          <span style={{ width: `${(data.items.length / data.max) * 100}%` }} />
        </div>
      </div>

      {editing && (
        <Panel
          title={editing === "new" ? t("Add achievement").toUpperCase() : t("Achievements").toUpperCase()}
        >
          <div className="ach-editor-grid">
            <div className="ach-editor-fields">
              <Field label={t("Name")} htmlFor="ach-name" error={fieldError("name")}>
                <input
                  id="ach-name"
                  className="pixel-input"
                  maxLength={40}
                  value={draft.name}
                  onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                />
              </Field>
              <Field label={t("Description")} htmlFor="ach-desc" error={fieldError("description")}>
                <input
                  id="ach-desc"
                  className="pixel-input"
                  maxLength={100}
                  value={draft.description}
                  onChange={(e) => setDraft({ ...draft, description: e.target.value })}
                />
              </Field>
              <div className="ach-two-fields">
                <Field label={t("Kind")} htmlFor="ach-kind" error={fieldError("kind")}>
                  <select
                    id="ach-kind"
                    className="pixel-input pixel-select"
                    value={draft.kind}
                    onChange={(e) => setDraft({ ...draft, kind: e.target.value })}
                  >
                    {data.kinds.map((kind) => (
                      <option key={kind} value={kind}>
                        {t(KIND_LABELS[kind] ?? kind)}
                      </option>
                    ))}
                  </select>
                </Field>
                <Field label={t("Target")} htmlFor="ach-target" error={fieldError("target")}>
                  <input
                    id="ach-target"
                    className="pixel-input"
                    type="number"
                    min={1}
                    max={1000000000}
                    value={draft.target}
                    disabled={draft.kind === "custom"}
                    onChange={(e) => setDraft({ ...draft, target: Number(e.target.value) })}
                  />
                </Field>
              </div>
              {off && (
                <p className="notice">
                  {t("This kind's module is off, so members will not see or earn it.")}
                </p>
              )}
              <Field label={t("Icon")} error={fieldError("icon")}>
                <div className="ach-icons" role="group" aria-label={t("Select an icon")}>
                  {[...data.icons, ...badgeIcons].map((icon) => (
                    <button
                      key={icon}
                      type="button"
                      className={`ach-icon ${draft.icon === icon ? "selected" : ""}`}
                      title={icon}
                      aria-label={icon}
                      aria-pressed={draft.icon === icon}
                      onClick={() => setDraft({ ...draft, icon })}
                    >
                      {icon.startsWith("b_") ? (
                        <AtlasThumb atlas={atlas.data} id={icon} size={32} />
                      ) : (
                        <span>{SYMBOLS[icon] ?? "★"}</span>
                      )}
                    </button>
                  ))}
                </div>
              </Field>
              <div className="ach-two-fields">
                <Field label={t("Role reward")} htmlFor="ach-role" error={fieldError("reward_role_id")}>
                  <RolePicker
                    id="ach-role"
                    info={discord.data}
                    value={draft.reward_role_id}
                    onChange={(v) => setDraft({ ...draft, reward_role_id: v })}
                    none={t("No role")}
                    assignable
                    forActions
                  />
                </Field>
                <Field label={t("Points reward")} htmlFor="ach-points" error={fieldError("reward_points")}>
                  <input
                    id="ach-points"
                    className="pixel-input"
                    type="number"
                    min={0}
                    max={1000000000}
                    value={draft.reward_points}
                    onChange={(e) => setDraft({ ...draft, reward_points: Number(e.target.value) })}
                  />
                </Field>
              </div>
              <Field label={t("Announce in")} htmlFor="ach-channel" error={fieldError("announce_channel_id")}>
                <ChannelPicker
                  id="ach-channel"
                  info={discord.data}
                  value={draft.announce_channel_id}
                  onChange={(v) => setDraft({ ...draft, announce_channel_id: v })}
                  none={t("No channel")}
                />
              </Field>
              <div className="ach-switches">
                <label>
                  {t("Secret until earned")}
                  <PixelToggle
                    label={t("Secret until earned")}
                    checked={draft.secret}
                    onChange={(v) => setDraft({ ...draft, secret: v })}
                  />
                </label>
                <label>
                  {t("Enabled")}
                  <PixelToggle
                    label={t("Enabled")}
                    checked={draft.enabled}
                    onChange={(v) => setDraft({ ...draft, enabled: v })}
                  />
                </label>
              </div>
              <div className="ach-actions">
                <PixelButton
                  onClick={submit}
                  disabled={save.isPending || !draft.name.trim() || draft.target < 1}
                >
                  {t("Save")}
                </PixelButton>
                <PixelButton variant="secondary" onClick={() => setPreview(!preview)}>
                  {t("Test preview")}
                </PixelButton>
                <PixelButton variant="ghost" onClick={() => setEditing(null)}>
                  {t("Cancel")}
                </PixelButton>
                {editing !== "new" && (
                  <PixelButton
                    variant="ghost"
                    onClick={() => removeItem(editing)}
                    disabled={remove.isPending}
                  >
                    {t("Delete")}
                  </PixelButton>
                )}
              </div>
              {save.isError && !error?.field && (
                <p className="field-error" role="alert">
                  {save.error.message}
                </p>
              )}
            </div>
            <div className="ach-live">
              <span className="pixel-label">{t("Preview")}</span>
              <AchievementPreview item={draft} t={t} atlas={atlas.data} earned={preview} />
              <p className="muted">
                {draft.secret ? t("Secret until earned") : t(KIND_LABELS[draft.kind] ?? draft.kind)} ·{" "}
                {draft.target}
              </p>
            </div>
          </div>
        </Panel>
      )}

      <div className="ach-list" aria-label={t("Achievements")}>
        {data.items.length === 0 && (
          <Panel title={t("Achievements").toUpperCase()}>
            <p className="empty-row muted">{t("No achievements yet. Add one to start a collection.")}</p>
          </Panel>
        )}
        {data.items.map((item) => (
          <article className={`ach-list-card ${item.enabled ? "" : "ach-disabled"}`} key={item.id}>
            <div className="ach-list-icon">
              <Icon icon={item.icon} atlas={atlas.data} />
            </div>
            <div className="ach-list-body">
              <div className="ach-list-top">
                <span className="pixel-label">
                  #{item.id} · {t(KIND_LABELS[item.kind] ?? item.kind)}
                </span>
                <strong>{item.target.toLocaleString(data.language)}</strong>
              </div>
              <h3>{item.name}</h3>
              <p>{item.description}</p>
              <div className="ach-tags">
                {item.secret && <span>{t("Secret until earned")}</span>}
                {item.reward_points > 0 && (
                  <span>
                    +{item.reward_points} {t("Points reward")}
                  </span>
                )}
                {item.module_off && (
                  <span className="ach-warning">
                    {t("This kind's module is off, so members will not see or earn it.")}
                  </span>
                )}
              </div>
            </div>
            <div className="ach-list-actions">
              <PixelButton variant="secondary" onClick={() => open(item)}>
                {t("Edit")}
              </PixelButton>
              <PixelButton variant="ghost" onClick={() => setHolders(holders === item.id ? null : item.id)}>
                {t("Who earned it")}
              </PixelButton>
            </div>
            {holders === item.id && (
              <div className="ach-holders">
                <span className="pixel-label">{t("Who earned it")}</span>
                {holdersQuery.isPending && <Loading />}
                {holdersQuery.isError && <Problem error={holdersQuery.error} />}
                {holdersQuery.data?.members.length === 0 && <p className="muted">{t("No one yet.")}</p>}
                {holdersQuery.data?.members.map((m) => (
                  <p key={m.user_id}>
                    <span>{m.user_id}</span>
                    <time>{new Date(m.earned_at).toLocaleDateString(data.language)}</time>
                  </p>
                ))}
              </div>
            )}
          </article>
        ))}
      </div>
      <Toast message={toast} onDone={closeToast} />
    </div>
  );
}

function Icon({ icon, atlas }: { icon: string; atlas: ReturnType<typeof useAtlasIndex>["data"] }) {
  return icon.startsWith("b_") ? (
    <AtlasThumb atlas={atlas} id={icon} size={44} />
  ) : (
    <span aria-hidden="true">{SYMBOLS[icon] ?? "★"}</span>
  );
}
function AchievementPreview({
  item,
  t,
  atlas,
  earned,
}: {
  item: AchievementDraft;
  t: (key: string) => string;
  atlas: ReturnType<typeof useAtlasIndex>["data"];
  earned: boolean;
}) {
  return (
    <div className={`ach-preview ${earned ? "ach-preview-earned" : ""}`}>
      <div className="ach-preview-icon">
        <Icon icon={item.icon} atlas={atlas} />
      </div>
      <span className="pixel-label">{earned ? t("Earned") : t("Locked")}</span>
      <h3>{item.name || t("Name")}</h3>
      <p>{item.description || t("Description")}</p>
      <div className="ach-preview-track">
        <span style={{ width: earned ? "100%" : "35%" }} />
      </div>
      <small>
        {earned ? item.target : Math.max(0, Math.round(item.target * 0.35))} / {item.target}
      </small>
    </div>
  );
}
