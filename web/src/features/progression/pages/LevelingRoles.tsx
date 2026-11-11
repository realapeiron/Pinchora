import { translate as spaTranslate } from "../../../i18n";
import { useState } from "react";
import { Field } from "../../../components/Field";
import { RolePicker, RoleTag } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { RichText } from "../../../components/RichText";
import { SettingsRow } from "../../../components/SettingsRow";
import { t } from "../../../i18n/dashboard";
import { levelingSummary } from "./levelingSummary";
import { ApiError, type GuildInfo } from "../../../lib/api";
import { LIMITS, checkLevel, problem } from "../../../lib/rules";
import { useLevelRoles, useStartSync, useJob } from "../../../lib/queries";
import { jobProgress, jobText, rememberJob, rememberedJob } from "../../../lib/syncJob";
import { Choice } from "../../../components/Choice";
/** The message of a failed list write, for the add form. */
function writeError(error: unknown): string | null {
  return error instanceof Error ? error.message : null;
}

export function LevelRoles({
  guild,
  info,
  stack,
  setStack,
}: {
  guild: string;
  info: GuildInfo | undefined;
  stack: boolean;
  setStack: (stack: boolean) => void;
}) {
  const { query, write } = useLevelRoles(guild);
  const [level, setLevel] = useState("5");
  const [role, setRole] = useState<string | null>(null);
  const picked = info?.roles.find((r) => r.id === role);
  const local = level === "" ? spaTranslate("Write a level.") : problem(checkLevel(Number(level)));
  const items = query.data?.items ?? [];
  const serverField = write.error instanceof ApiError ? write.error.field : undefined;

  return (
    <SettingsRow
      rowId="leveling-roles-level-roles"
      title={t("row.Level roles", {}, "LEVEL ROLES")}
      description={t("Give roles when members reach a level.")}
      summary={levelingSummary({}, { levelRoles: items.length }).levelRoles}
    >
      <div className="settings-detail-fields">
        <span className="pixel-label muted">
          {t("{count} of {max} level roles used", {
            count: items.length,
            max: query.data?.max ?? LIMITS.max_level_roles,
          })}
        </span>
        <Field label={spaTranslate("When a member earns a new one")}>
          <Choice
            name="Level role mode"
            value={stack ? "stack" : "replace"}
            options={[
              {
                value: "stack",
                title: spaTranslate("Stack"),
                detail: spaTranslate("Keep every role earned."),
              },
              {
                value: "replace",
                title: spaTranslate("Replace"),
                detail: spaTranslate("Keep only the highest one."),
              },
            ]}
            onChange={(mode) => setStack(mode === "stack")}
          />
        </Field>
        {items.length === 0 ? (
          <p className="empty-row muted">{spaTranslate("No level roles yet.")}</p>
        ) : (
          <ul className="list" aria-label={spaTranslate("Level roles")}>
            {items.map((row) => (
              <li key={row.role_id} className="list-row">
                <span className="list-level pixel-label">
                  {spaTranslate("LV")} {row.level}
                </span>
                <span className="list-main">
                  <RoleTag info={info} id={row.role_id} />
                </span>
                <PixelButton
                  variant="ghost"
                  className="button-small"
                  aria-label={spaTranslate("Remove level role")}
                  disabled={write.isPending}
                  onClick={() => write.mutate({ method: "DELETE", sub: `/${row.role_id}` })}
                >
                  {spaTranslate("Remove")}
                </PixelButton>
              </li>
            ))}
          </ul>
        )}
        <div className="add-row add-row-role">
          <Field
            label={spaTranslate("Level")}
            htmlFor="level-role-level"
            error={local ?? (serverField === "level" ? writeError(write.error) : null)}
          >
            <input
              id="level-role-level"
              className="pixel-input input-short"
              type="number"
              min={LIMITS.level[0]}
              max={LIMITS.level[1]}
              value={level}
              onChange={(event) => setLevel(event.target.value)}
            />
          </Field>
          <Field
            label={spaTranslate("Role")}
            htmlFor="level-role-role"
            error={
              (picked && !picked.assignable ? picked.why_not : null) ??
              (serverField === "role_id" ? writeError(write.error) : null)
            }
          >
            <RolePicker id="level-role-role" info={info} value={role} onChange={setRole} assignable />
          </Field>
          <div className="add-actions">
            <PixelButton
              disabled={!role || !!local || (picked && !picked.assignable) || write.isPending}
              onClick={() =>
                role &&
                write.mutate(
                  { method: "PUT", sub: `/${role}`, body: { level: Number(level) } },
                  {
                    onSuccess: (list) => {
                      if (list.items.some((item) => item.role_id === role)) setRole(null);
                    },
                  },
                )
              }
            >
              {" "}
              {spaTranslate("Add")}{" "}
            </PixelButton>
          </div>
        </div>
        {write.isError && !serverField && (
          <p className="field-error" role="alert">
            <RichText text={writeError(write.error) ?? ""} />
          </p>
        )}
        <SyncMembers guild={guild} disabled={items.length === 0} />
      </div>
    </SettingsRow>
  );
}

/** Gives members already past a level the roles they earned; shows the job's progress. */
function SyncMembers({ guild, disabled }: { guild: string; disabled: boolean }) {
  const start = useStartSync(guild);
  // A reload (or a second tab) picks up the job this browser started last, if it is still running.
  const [job, setJob] = useState<string | null>(() => rememberedJob(guild));
  const [started, setStarted] = useState(false);
  const state = useJob(guild, job);
  const follow = (id: string) => {
    rememberJob(guild, id);
    setJob(id);
  };
  const running = start.isPending || state.data?.status === "running";
  // A job that ended before this page opened is not shown again; one started here stays until a reload.
  const showing = state.data !== undefined && (running || started);
  const progress = jobProgress(state.data);
  return (
    <div className="sync">
      <div className="sync-text">
        <h3 className="setting-name">{spaTranslate("Sync members")}</h3>
        <p className="muted">
          {" "}
          {spaTranslate(
            "Give members who are already past a level the roles they earned. Checks up to 1,000 members with the newest XP first.",
          )}{" "}
        </p>
      </div>
      <PixelButton
        variant="secondary"
        disabled={disabled || running}
        onClick={() => {
          setStarted(true);
          start.mutate(undefined, {
            onSuccess: (r) => follow(r.job_id),
            // Already running (here or in another tab): follow that one instead.
            onError: (error) => {
              if (error instanceof ApiError && error.jobId) follow(error.jobId);
            },
          });
        }}
      >
        {running ? spaTranslate("Syncing…") : spaTranslate("Sync members")}
      </PixelButton>
      {(showing || start.isPending) && (
        <div className="sync-status" aria-live="polite">
          <div
            className="meter"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(progress * 100)}
          >
            <span className="meter-fill" style={{ width: `${Math.round(progress * 100)}%` }} />
          </div>
          <p>{state.data?.message ? <RichText text={state.data.message} /> : jobText(state.data)}</p>
        </div>
      )}
      {start.isError && !(start.error instanceof ApiError && start.error.jobId) && (
        <p className="field-error" role="alert">
          {start.error.message}
        </p>
      )}
    </div>
  );
}
