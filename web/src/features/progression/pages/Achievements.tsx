import { useCallback, useState } from "react";
import { date, number, serverText } from "../../../i18n";
import { useOutletContext } from "react-router";

import { PixelButton } from "../../../components/PixelButton";
import { Toast } from "../../../components/Toast";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import {
  useAchievements,
  useAchievementHolders,
  type Achievement,
  type AchievementDraft,
} from "../api/achievements";
import { type Overview } from "../../../lib/api";
import { useAtlasIndex, useDiscord } from "../../../lib/queries";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import { fresh, KIND_LABELS } from "./achievementLabels";
import { AchievementEditor } from "./AchievementEditor";
import { Icon } from "./AchievementPresentation";
import { achievementSummary, achievementsSummary } from "./achievementsSummary";
import "../../../styles/achievements.css";

export function Achievements() {
  const overview = useOutletContext<Overview>();
  const guild = overview.guild.id;
  const { query, save, remove } = useAchievements(guild);
  const discord = useDiscord(guild);
  const atlas = useAtlasIndex();
  const [editing, setEditing] = useState<string | "new" | null>(null);
  const [draft, setDraft] = useState<AchievementDraft>(fresh);
  const [holders, setHolders] = useState<string | null>(null);
  const holdersQuery = useAchievementHolders(guild, holders);
  const [toast, setToast] = useState<string | null>(null);
  const closeToast = useCallback(() => setToast(null), []);

  if (query.isPending) return <Loading />;
  if (query.isError) return <Problem error={query.error} />;
  const data = query.data;
  const text = (key: string) => serverText(key, data.strings);
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
    save.reset();
  };
  const submit = () =>
    save.mutate(
      { id: editing === "new" ? undefined : (editing ?? undefined), draft },
      {
        onSuccess: () => {
          setEditing(null);
          setToast(text("Saved."));
        },
      },
    );
  const removeItem = (id: string) =>
    remove.mutate(id, {
      onSuccess: () => {
        setEditing(null);
        setHolders(null);
        setToast(text("Removed."));
      },
    });
  return (
    <SettingsPage
      className="settings-page ach-page"
      title={text("ACHIEVEMENTS")}
      lead={text("Create milestones that celebrate what members do in your server.")}
      action={
        <PixelButton onClick={() => open()} disabled={data.items.length >= data.max}>
          {text("Add achievement")}
        </PixelButton>
      }
    >
      <div className="ach-summary">
        <span className="pixel-label">{text("Achievements")}</span>
        <strong>
          {data.items.length}
          <span>/{data.max}</span>
        </strong>
        <div className="ach-summary-track">
          <span style={{ width: `${(data.items.length / data.max) * 100}%` }} />
        </div>
      </div>

      {editing && (
        <AchievementEditor
          key={editing}
          editing={editing}
          draft={draft}
          setDraft={setDraft}
          data={data}
          discord={discord.data}
          atlas={atlas.data}
          save={save}
          remove={remove}
          modules={overview.modules}
          onSubmit={submit}
          onCancel={() => setEditing(null)}
          onRemove={removeItem}
          t={text}
        />
      )}

      <div className="ach-list settings-detail-list" aria-label={text("Achievements")}>
        {data.items.length === 0 && (
          <SettingsRow
            title={text("Achievements")}
            description={dt("Manage milestones and rewards.")}
            summary={achievementsSummary(data.items)}
            initialOpen
          >
            <p className="empty-row muted">{text("No achievements yet. Add one to start a collection.")}</p>
          </SettingsRow>
        )}
        {data.items.map((item) => (
          <SettingsRow
            key={item.id}
            title={item.name}
            description={dt("Review this achievement.")}
            summary={achievementSummary(item)}
          >
            <article className={`ach-list-card ${item.enabled ? "" : "ach-disabled"}`} key={item.id}>
              <div className="ach-list-icon">
                <Icon icon={item.icon} atlas={atlas.data} />
              </div>
              <div className="ach-list-body">
                <div className="ach-list-top">
                  <span className="pixel-label">
                    #{item.id} · {text(KIND_LABELS[item.kind] ?? item.kind)}
                  </span>
                  <strong>{number(item.target)}</strong>
                </div>
                <p>{item.description}</p>
                <div className="ach-tags">
                  {item.secret && <span>{text("Secret until earned")}</span>}
                  {item.reward_points > 0 && (
                    <span>
                      +{number(item.reward_points)} {text("Points reward")}
                    </span>
                  )}
                  {item.module_off && (
                    <span className="ach-warning">
                      {text("This kind's module is off, so members will not see or earn it.")}
                    </span>
                  )}
                </div>
              </div>
              <div className="ach-list-actions">
                <PixelButton variant="secondary" onClick={() => open(item)}>
                  {text("Edit")}
                </PixelButton>
                <PixelButton variant="ghost" onClick={() => setHolders(holders === item.id ? null : item.id)}>
                  {text("Who earned it")}
                </PixelButton>
              </div>
              {holders === item.id && (
                <div className="ach-holders">
                  <span className="pixel-label">{text("Who earned it")}</span>
                  {holdersQuery.isPending && <Loading />}
                  {holdersQuery.isError && <Problem error={holdersQuery.error} />}
                  {holdersQuery.data?.members.length === 0 && <p className="muted">{text("No one yet.")}</p>}
                  {holdersQuery.data?.members.map((m) => (
                    <p key={m.user_id}>
                      <span>{m.user_id}</span>
                      <time>{date(m.earned_at)}</time>
                    </p>
                  ))}
                </div>
              )}
            </article>
          </SettingsRow>
        ))}
      </div>
      <Toast message={toast} onDone={closeToast} />
    </SettingsPage>
  );
}
