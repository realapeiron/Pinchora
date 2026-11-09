import { useState } from "react";
import "../../../styles/moderation-desk.css";
import { Link, useOutletContext, useParams } from "react-router";
import { useMutation, useQuery } from "@tanstack/react-query";
import { translate as t } from "../../../i18n";
import { api, type Overview } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { useModerationDesk, policyChanged, type ModPolicy, type QueueRow } from "../api/moderationDesk";
import { Panel } from "../../../components/Panel";
import { Field } from "../../../components/Field";
import { ChannelPicker, RolePicker } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { PageHead, SettingsPage } from "../../../components/PageHead";
import { RequireLogin } from "../../../components/RequireLogin";
import { Loading, Problem } from "../../../components/States";

export function ModerationDesk() {
  const { guild } = useOutletContext<Overview>();
  const { query } = useModerationDesk(guild.id);
  return (
    <SettingsPage
      className="settings-page mod-parity"
      title={t("Moderation desk")}
      lead={t("Review member reports and appeals, and configure restrictions and role rules.")}
    >
      {query.isPending && <Loading />}
      {query.isError && <Problem error={query.error} />}
      {query.data && (
        <Desk
          guild={guild.id}
          saved={query.data.config}
          reports={query.data.queues.reports}
          appeals={query.data.queues.appeals}
        />
      )}
    </SettingsPage>
  );
}
function Desk({
  guild,
  saved,
  reports,
  appeals,
}: {
  guild: string;
  saved: ModPolicy;
  reports: QueueRow[];
  appeals: QueueRow[];
}) {
  const [draft, setDraft] = useState(saved);
  const [reason, setReason] = useState("");
  const [selected, setSelected] = useState<{ queue: string; row: QueueRow; accept: boolean } | null>(null);
  const { save, preview, review } = useModerationDesk(guild);
  const discord = useDiscord(guild);
  const change = <K extends keyof ModPolicy>(key: K, value: ModPolicy[K]) =>
    setDraft({ ...draft, [key]: value });
  const error = save.error ?? preview.error ?? review.error;
  return (
    <>
      <Panel title={t("Staff inbox")}>
        <Field label={t("Report channel")} htmlFor="report-inbox">
          <ChannelPicker
            id="report-inbox"
            info={discord.data}
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
            <RolePicker id={key} info={discord.data} value={draft[key]} onChange={(v) => change(key, v)} />
          </Field>
        ))}
        <p className="muted">
          {t("Saved roles are restored on release only when they remain safe and below staff and bot roles.")}
        </p>
      </Panel>
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
                info={discord.data}
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
                info={discord.data}
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
                info={discord.data}
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
      {error && <Problem error={error} />}
      <div className="save-bar">
        <PixelButton
          disabled={save.isPending || !policyChanged(saved, draft)}
          onClick={() => save.mutate(draft)}
        >
          {t("Save changes")}
        </PixelButton>{" "}
        <PixelButton variant="secondary" disabled={preview.isPending} onClick={() => preview.mutate(draft)}>
          {t("Validate policy")}
        </PixelButton>{" "}
        <PixelButton variant="ghost" onClick={() => setDraft(saved)}>
          {t("Discard")}
        </PixelButton>
      </div>
      {save.isSuccess && <p role="status">{t("Policy saved.")}</p>}
      {preview.isSuccess && <p role="status">{t("Policy checks passed.")}</p>}
      {(
        [
          ["reports", reports, "Member reports"],
          ["appeals", appeals, "Member appeals"],
        ] as const
      ).map(([queue, rows, title]) => (
        <Panel key={queue} title={t(title)}>
          {!rows.length && <p className="empty-row">{t("The review queue is clear.")}</p>}
          <ul className="list">
            {rows.map((row) => (
              <li key={row.id} className="list-row">
                <div>
                  <strong>
                    #{row.id} · {row.user_id}
                  </strong>
                  <p>{row.body}</p>
                  {row.case_number && (
                    <p>
                      {t("Cases")} #{row.case_number}
                    </p>
                  )}
                  {row.reporter_id && (
                    <p>
                      {t("Reported by")}: {row.reporter_id}
                    </p>
                  )}
                  {row.channel_id && row.message_id && (
                    <p>
                      <a
                        href={`https://discord.com/channels/${guild}/${row.channel_id}/${row.message_id}`}
                        target="_blank"
                        rel="noreferrer"
                      >
                        {t("Open reported message")}
                      </a>
                    </p>
                  )}
                  <span>{t(row.status)}</span>
                </div>
                {row.status === "pending" && (
                  <div>
                    <PixelButton
                      variant="secondary"
                      onClick={() => {
                        setSelected({ queue, row, accept: true });
                        setReason("");
                      }}
                    >
                      {t("Accept")}
                    </PixelButton>{" "}
                    <PixelButton
                      variant="ghost"
                      onClick={() => {
                        setSelected({ queue, row, accept: false });
                        setReason("");
                      }}
                    >
                      {t("Reject")}
                    </PixelButton>
                  </div>
                )}
              </li>
            ))}
          </ul>
        </Panel>
      ))}
      {selected && (
        <Panel title={t("Confirm review")}>
          <p>
            #{selected.row.id} · {selected.row.user_id}
          </p>
          <p>
            {selected.queue === "reports"
              ? t("Record the outcome of this report for staff.")
              : selected.accept
                ? t("Accepting an appeal reverses the active ban or warn.")
                : t("The member will see this decision.")}
          </p>
          <Field label={t("Decision reason")} htmlFor="review-reason">
            <textarea
              /* eslint-disable-next-line jsx-a11y/no-autofocus -- The decision dialog focuses its reason field. */
              autoFocus
              id="review-reason"
              className="pixel-input"
              maxLength={512}
              value={reason}
              onChange={(e) => setReason(e.target.value)}
            />
          </Field>
          <PixelButton
            disabled={!reason.trim() || review.isPending}
            onClick={() =>
              review.mutate(
                { queue: selected.queue, id: selected.row.id, accept: selected.accept, reason },
                { onSuccess: () => setSelected(null) },
              )
            }
          >
            {t("Confirm decision")}
          </PixelButton>{" "}
          <PixelButton variant="ghost" onClick={() => setSelected(null)}>
            {t("Cancel")}
          </PixelButton>
        </Panel>
      )}
    </>
  );
}
export function AppealForm() {
  const { guild } = useParams();
  return <RequireLogin>{() => (guild ? <MemberAppeal /> : <AppealCases />)}</RequireLogin>;
}
function MemberAppeal() {
  const { guild, caseNumber } = useParams();
  const path = `/api/appeals/${guild}/${caseNumber}`;
  const [body, setBody] = useState("");
  const query = useQuery({
    queryKey: ["appeal", guild, caseNumber],
    queryFn: () =>
      api<{
        kind: string;
        reason: string | null;
        active: boolean;
        appeal: { status: string; decision: string | null } | null;
      }>(path),
    retry: false,
  });
  const submit = useMutation({ mutationFn: () => api(path, "POST", { body }) });
  return (
    <main className="settings-page mod-parity">
      <PageHead
        title={t("Appeal a moderation case")}
        lead={t("Explain what happened and why staff should reconsider this case.")}
      />
      {query.isPending && <Loading />}
      {query.isError && <Problem error={query.error} />}
      {query.data && (
        <Panel title={`#${caseNumber}`}>
          <p>
            {t(query.data.kind)} · {query.data.reason}
          </p>
          {query.data.appeal ? (
            <div role="status">
              <p>{t(query.data.appeal.status)}</p>
              <p>{query.data.appeal.decision}</p>
            </div>
          ) : submit.isSuccess ? (
            <p role="status">{t("Your appeal is waiting for staff review.")}</p>
          ) : (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                submit.mutate();
              }}
            >
              <Field label={t("Your appeal")} htmlFor="appeal-body">
                <textarea
                  id="appeal-body"
                  className="pixel-input"
                  minLength={10}
                  maxLength={2000}
                  required
                  value={body}
                  onChange={(e) => setBody(e.target.value)}
                />
              </Field>
              <p>{body.length}/2000</p>
              {submit.isError && <Problem error={submit.error} />}
              <PixelButton
                type="submit"
                disabled={!query.data.active || body.trim().length < 10 || submit.isPending}
              >
                {t("Submit appeal")}
              </PixelButton>
            </form>
          )}
        </Panel>
      )}
    </main>
  );
}

function AppealCases() {
  const cases = useQuery({
    queryKey: ["my-appeals"],
    queryFn: () =>
      api<{ cases: { guild: string; number: number; kind: string; reason: string | null }[] }>(
        "/api/appeals",
      ),
  });
  return (
    <main className="settings-page mod-parity">
      <PageHead
        title={t("Member appeals")}
        lead={t("Explain what happened and why staff should reconsider this case.")}
      />
      <a href="/api/appeals/export" download="moderation-data.json">
        {t("Download moderation data")}
      </a>
      {cases.isPending && <Loading />}
      {cases.isError && <Problem error={cases.error} />}
      {cases.data && (
        <Panel title={t("Your cases")}>
          <ul className="list">
            {cases.data.cases.map((c) => (
              <li key={`${c.guild}-${c.number}`} className="list-row">
                <Link to={`/appeals/${c.guild}/${c.number}`}>
                  #{c.number} · {t(c.kind)} · {c.guild}
                </Link>
                <p>{c.reason}</p>
              </li>
            ))}
          </ul>
          {!cases.data.cases.length && <p>{t("No cases are open for appeal.")}</p>}
        </Panel>
      )}
    </main>
  );
}
