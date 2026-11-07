import { InviteWordsContext, useInviteWords } from "../lib/inviteWords";
import "../styles/invites.css";

import { useCallback, useState } from "react";
import { Link, useOutletContext } from "react-router";

import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { ChannelPicker, RolePicker } from "../components/Pickers";
import { PixelButton, PixelLink } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import { ApiError, type Overview } from "../lib/api";
import {
  type InvitePeriod,
  type InviteSettings,
  inviteExportUrl,
  useInviteLeaders,
  useInviteMember,
  useInvites,
} from "../lib/invites";
import { useDiscord } from "../lib/queries";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

export function Invites() {
  const data = useOutletContext<Overview>();
  const invites = useInvites(data.guild.id);
  if (invites.query.isPending) return <Loading />;
  if (invites.query.isError) return <Problem error={invites.query.error} />;
  return (
    <InviteWordsContext.Provider value={invites.query.data}>
      <InvitesPage />
    </InviteWordsContext.Provider>
  );
}

function InvitesPage() {
  const { t } = useInviteWords();
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const invites = useInvites(guild);
  const [toast, setToast] = useState<string | null>(null);
  const closeToast = useCallback(() => setToast(null), []);
  const module = data.modules.find((m) => m.key === "invites");
  return (
    <div className="settings-page inv-page">
      <PageHead
        title={t("INVITES")}
        lead={t(
          "See where new members came from, reward real invitations, and give members short-lived links for friends.",
        )}
        action={
          <div className="inv-head-actions">
            <PixelButton
              variant="secondary"
              onClick={() => {
                void invites.query
                  .refetch()
                  .then(({ data }) =>
                    setToast(
                      data?.can_read_invites
                        ? t("Invite list is available.")
                        : t("Manage Server permission is missing for the bot."),
                    ),
                  );
              }}
            >
              {t("Test tracking")}
            </PixelButton>
            <PixelLink href={inviteExportUrl(guild)} variant="secondary">
              {t("Export CSV")}
            </PixelLink>
          </div>
        }
      />
      {module && !module.enabled && (
        <p className="notice" role="note">
          <strong>{t("Invite tracking is off.")}</strong> {t("Switch it on in")}{" "}
          <Link to={`/g/${guild}/modules`}>{t("Modules")}</Link> {t("to count new joins.")}
        </p>
      )}
      {invites.query.isPending && <Loading label={t("Reading invite settings")} />}
      {invites.query.isError && <Problem error={invites.query.error} />}
      {invites.query.data && (
        <>
          {!invites.query.data.can_read_invites && (
            <p className="notice" role="note">
              <strong>{t("Manage Server permission missing.")}</strong>{" "}
              {t(
                "Pinchora needs Manage Server to read the invite list. New joins stay unknown until the bot has it.",
              )}
            </p>
          )}
          <div className="inv-overview">
            <div className="inv-overview-mark" aria-hidden="true">
              ↗
            </div>
            <div>
              <strong>{t("Every join has a source.")}</strong>
              <p>
                {t(
                  "Invite uses are compared when someone joins. If Discord cannot identify a code, the join stays unknown; it is never credited by guesswork.",
                )}
              </p>
            </div>
          </div>
          <InviteSettingsForm
            guild={guild}
            saved={invites.query.data.settings}
            save={invites.save}
            onToast={setToast}
          />
          <Rewards
            guild={guild}
            rewards={invites.query.data.rewards}
            limit={invites.query.data.limits.rewards}
            reward={invites.reward}
            remove={invites.remove}
            onToast={setToast}
          />
          <Leaderboard guild={guild} />
        </>
      )}
      <Toast message={toast} onDone={closeToast} />
    </div>
  );
}

function InviteSettingsForm({
  guild,
  saved,
  save,
  onToast,
}: {
  guild: string;
  saved: InviteSettings;
  save: ReturnType<typeof useInvites>["save"];
  onToast: (s: string) => void;
}) {
  const { t } = useInviteWords();
  const discord = useDiscord(guild);
  const [draft, setDraft] = useState(saved);
  const set = <K extends keyof InviteSettings>(key: K, value: InviteSettings[K]) =>
    setDraft((old) => ({ ...old, [key]: value }));
  const fields: (keyof InviteSettings)[] = [
    "enabled",
    "log_channel_id",
    "fake_days",
    "temp_enabled",
    "temp_channel_id",
    "temp_max_age_secs",
    "temp_max_uses",
    "temp_per_user_daily",
  ];
  const count = fields.filter((key) => draft[key] !== saved[key]).length;
  const error = save.error instanceof ApiError ? save.error : null;
  const problem = (key: string) => (error?.field === key ? error.message : null);
  return (
    <>
      <div className="inv-settings-grid">
        <Panel title={t("TRACKING")}>
          <SwitchRow
            title={t("Count invitations")}
            control={
              <PixelToggle
                label={t("Count invitations")}
                checked={draft.enabled}
                onChange={(v) => set("enabled", v)}
              />
            }
          >
            {t("Count new members when their invite code can be identified.")}
          </SwitchRow>
          <Field
            label={t("Join log channel")}
            htmlFor="inv-log"
            hint={t("Optional. Keep it visible only to the people who should review joins.")}
            error={problem("log_channel_id")}
          >
            <ChannelPicker
              id="inv-log"
              info={discord.data}
              value={draft.log_channel_id}
              onChange={(v) => set("log_channel_id", v)}
              none={t("No dedicated join log")}
            />
          </Field>
          <Field
            label={t("New account window")}
            htmlFor="inv-fake"
            hint={t(
              "Members with younger Discord accounts are shown separately and do not earn invite rewards.",
            )}
            error={problem("fake_days")}
          >
            <div className="inv-unit">
              <input
                id="inv-fake"
                type="number"
                min="0"
                max="365"
                className="pixel-input"
                value={draft.fake_days}
                onChange={(e) => set("fake_days", Number(e.target.value))}
              />
              <span>{t("days")}</span>
            </div>
          </Field>
        </Panel>
        <Panel title={t("FRIEND LINKS")}>
          <SwitchRow
            title={t("Let members make links")}
            control={
              <PixelToggle
                label={t("Let members make links")}
                checked={draft.temp_enabled}
                onChange={(v) => set("temp_enabled", v)}
              />
            }
          >
            {t("Each link is sent privately to the member who requests it.")}
          </SwitchRow>
          <Field
            label={t("Invite channel")}
            htmlFor="inv-temp-channel"
            hint={t("The bot needs Create Instant Invite in this channel.")}
            error={problem("temp_channel_id")}
          >
            <ChannelPicker
              id="inv-temp-channel"
              info={discord.data}
              value={draft.temp_channel_id}
              onChange={(v) => set("temp_channel_id", v)}
              none={t("Choose a channel")}
            />
          </Field>
          <div className="inv-number-grid">
            <Field label={t("Link lifetime")} htmlFor="inv-age" error={problem("temp_max_age_secs")}>
              <div className="inv-unit">
                <input
                  id="inv-age"
                  type="number"
                  min="60"
                  max="604800"
                  className="pixel-input"
                  value={draft.temp_max_age_secs}
                  onChange={(e) => set("temp_max_age_secs", Number(e.target.value))}
                />
                <span>{t("sec")}</span>
              </div>
            </Field>
            <Field label={t("Uses per link")} htmlFor="inv-uses" error={problem("temp_max_uses")}>
              <input
                id="inv-uses"
                type="number"
                min="1"
                max="100"
                className="pixel-input"
                value={draft.temp_max_uses}
                onChange={(e) => set("temp_max_uses", Number(e.target.value))}
              />
            </Field>
            <Field label={t("Per member daily")} htmlFor="inv-daily" error={problem("temp_per_user_daily")}>
              <input
                id="inv-daily"
                type="number"
                min="1"
                max="20"
                className="pixel-input"
                value={draft.temp_per_user_daily}
                onChange={(e) => set("temp_per_user_daily", Number(e.target.value))}
              />
            </Field>
          </div>
          <p className="muted inv-small-note">
            {t("Members use")} <code>/invites temp</code>{" "}
            {t("or the “Invite a friend” button. Post the button with")} <code>/setup invites post</code>.
          </p>
        </Panel>
      </div>
      <SaveBar
        words={t}
        count={count}
        saving={save.isPending}
        error={error?.message}
        blocked={draft.temp_enabled && !draft.temp_channel_id}
        onReset={() => {
          setDraft(saved);
          save.reset();
        }}
        onSave={() =>
          save.mutate(draft, {
            onSuccess: (now) => {
              setDraft(now);
              onToast(t("Invite settings saved."));
            },
          })
        }
      />
    </>
  );
}

function Rewards({
  guild,
  rewards,
  limit,
  reward,
  remove,
  onToast,
}: {
  guild: string;
  rewards: { invites: number; role_id: string }[];
  limit: number;
  reward: ReturnType<typeof useInvites>["reward"];
  remove: ReturnType<typeof useInvites>["remove"];
  onToast: (s: string) => void;
}) {
  const { t } = useInviteWords();
  const discord = useDiscord(guild);
  const [at, setAt] = useState(5);
  const [role, setRole] = useState<string | null>(null);
  const error = reward.error instanceof ApiError ? reward.error : null;
  return (
    <Panel
      title={t("REWARD ROLES")}
      aside={
        <span className="badge badge-gold">
          {t("{count}/{limit} used", { count: rewards.length, limit })}
        </span>
      }
    >
      <p className="panel-lead muted">
        {t(
          "A member earns a role when enough of their invitees are still here with eligible accounts. The role is checked again before Pinchora gives it.",
        )}
      </p>
      {rewards.length === 0 ? (
        <p className="empty-row muted">{t("No milestones yet. Add the first one below.")}</p>
      ) : (
        <ul className="list" aria-label={t("Invite rewards")}>
          {rewards.map((r) => (
            <li className="list-row inv-reward" key={r.invites}>
              <span className="inv-reward-count">{t("{count} invites", { count: r.invites })}</span>
              <span>→</span>
              <strong>
                {discord.data?.roles.find((role) => role.id === r.role_id)?.name ??
                  t("Role {id}", { id: r.role_id })}
              </strong>
              <PixelButton
                variant="ghost"
                aria-label={`Remove reward at ${t("{count} invites", { count: r.invites })}`}
                onClick={() => remove.mutate(r.invites, { onSuccess: () => onToast(t("Reward removed.")) })}
              >
                {t("Remove")}
              </PixelButton>
            </li>
          ))}
        </ul>
      )}
      <div className="inv-add-reward">
        <Field
          label={t("Invites needed")}
          htmlFor="inv-threshold"
          error={error?.field === "invites" ? error.message : null}
        >
          <input
            id="inv-threshold"
            type="number"
            min="1"
            max="1000000"
            className="pixel-input"
            value={at}
            onChange={(e) => setAt(Number(e.target.value))}
          />
        </Field>
        <Field label={t("Role")} htmlFor="inv-role" error={error?.field === "role_id" ? error.message : null}>
          <RolePicker
            id="inv-role"
            info={discord.data}
            value={role}
            onChange={setRole}
            assignable
            none={t("Choose a role")}
          />
        </Field>
        <PixelButton
          disabled={!role || at < 1 || rewards.length >= limit || reward.isPending}
          onClick={() =>
            role &&
            reward.mutate(
              { invites: at, role_id: role },
              {
                onSuccess: () => {
                  setRole(null);
                  onToast(t("Reward saved."));
                },
              },
            )
          }
        >
          {t("Add reward")}
        </PixelButton>
      </div>
    </Panel>
  );
}

function Leaderboard({ guild }: { guild: string }) {
  const { t, n } = useInviteWords();
  const [period, setPeriod] = useState<InvitePeriod>("all");
  const [page, setPage] = useState(0);
  const [typed, setTyped] = useState("");
  const [member, setMember] = useState<string | null>(null);
  const leaders = useInviteLeaders(guild, period, page);
  const detail = useInviteMember(guild, member);
  const choose = (next: InvitePeriod) => {
    setPeriod(next);
    setPage(0);
  };
  return (
    <Panel
      title={t("LEADERBOARD")}
      aside={
        <div className="inv-tabs" role="group" aria-label={t("Leaderboard period")}>
          {(["all", "week", "month"] as const).map((key) => (
            <button type="button" key={key} aria-pressed={period === key} onClick={() => choose(key)}>
              {key === "all" ? t("All time") : key === "week" ? t("7 days") : t("30 days")}
            </button>
          ))}
        </div>
      }
    >
      {leaders.isPending && <Loading label={t("Reading leaderboard")} />}
      {leaders.isError && <Problem error={leaders.error} />}
      {leaders.data && (
        <>
          {leaders.data.rows.length === 0 ? (
            <p className="empty-row muted">{t("No counted invitations in this period yet.")}</p>
          ) : (
            <div className="inv-table-wrap">
              <table className="inv-table">
                <thead>
                  <tr>
                    <th>{t("Rank")}</th>
                    <th>{t("Member")}</th>
                    <th>{t("Regular")}</th>
                    <th>{t("Left")}</th>
                    <th>{t("Too new")}</th>
                    <th>{t("Bonus")}</th>
                    <th>{t("Counted")}</th>
                  </tr>
                </thead>
                <tbody>
                  {leaders.data.rows.map((r, i) => (
                    <tr key={r.user_id}>
                      <td>{n(page * 25 + i + 1)}</td>
                      <td>
                        <button
                          className="inv-member-link"
                          onClick={() => {
                            setMember(r.user_id);
                            setTyped(r.user_id);
                          }}
                        >
                          {r.user_id}
                        </button>
                      </td>
                      <td>{n(r.regular)}</td>
                      <td>{n(r.left)}</td>
                      <td>{n(r.fake)}</td>
                      <td>{n(r.bonus)}</td>
                      <td className="inv-total">{n(r.total)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <div className="inv-pagination">
            <PixelButton variant="secondary" disabled={page === 0} onClick={() => setPage(page - 1)}>
              {t("Previous")}
            </PixelButton>
            <span>{t("Page {page}", { page: page + 1 })}</span>
            <PixelButton
              variant="secondary"
              disabled={leaders.data.rows.length < 25}
              onClick={() => setPage(page + 1)}
            >
              {t("Next")}
            </PixelButton>
          </div>
        </>
      )}
      <div className="inv-detail">
        <h3>{t("Member detail")}</h3>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (/^\d{15,22}$/.test(typed)) setMember(typed);
          }}
        >
          <input
            className="pixel-input"
            aria-label={t("Discord user ID")}
            placeholder={t("Discord user ID")}
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
          />
          <PixelButton type="submit">{t("Look up")}</PixelButton>
        </form>
        {detail.isPending && member && <Loading label={t("Reading member invites")} />}
        {detail.isError && <Problem error={detail.error} />}
        {detail.data && (
          <>
            <p className="inv-detail-total">
              {t("{count} counted invites for {member}", {
                count: detail.data.score.total,
                member: member ?? "",
              })}
            </p>
            <ul className="inv-join-list">
              {detail.data.joins.map((j, i) => (
                <li key={`${j.user_id}-${i}`}>
                  <span>{j.user_id}</span>
                  <span>{new Date(j.joined_at).toLocaleDateString()}</span>
                  <span className={`inv-source ${j.source}`}>
                    {j.source === "temporary" ? t("possible used-up link") : t(j.source)}
                    {j.fake ? t(" · new account") : ""}
                    {j.left_at ? t(" · left") : ""}
                  </span>
                </li>
              ))}
            </ul>
            {detail.data.joins.length === 0 && (
              <p className="muted">{t("No joins credited to this member.")}</p>
            )}
          </>
        )}
      </div>
    </Panel>
  );
}
