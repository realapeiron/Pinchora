import { useState } from "react";
import { useOutletContext } from "react-router";

import { Choice } from "../components/Choice";
import { Field, SwitchRow } from "../components/Field";
import { MessageField } from "../components/MessageField";
import { Panel } from "../components/Panel";
import { ChannelPicker, ChannelTag, RolePicker, RoleTag } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { RichText } from "../components/RichText";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import { ApiError, type FieldError, type GuildInfo, type Overview, type Settings } from "../lib/api";
import { useDiscord, useJob, useLevelRoles, useRepRoles, useStartSync, useXpRules } from "../lib/queries";
import {
  LIMITS,
  checkLevel,
  checkMessage,
  checkMultiplier,
  checkReputation,
  checkVoiceAmount,
  problem,
} from "../lib/rules";
import { jobProgress, jobText, rememberJob, rememberedJob } from "../lib/syncJob";
import { useSettingsForm } from "../lib/useSettingsForm";
import { PageHead } from "./PageHead";
import { XpAdmin } from "./XpAdmin";

const MODES = [
  {
    value: "channel",
    title: "Level-up channel",
    detail: "The channel below, or where they chatted if none.",
  },
  { value: "current", title: "Where they chatted", detail: "Always the channel the member was talking in." },
  { value: "dm", title: "Direct message", detail: "A private message to the member." },
  { value: "off", title: "Off", detail: "No message. Level roles are still given." },
];

function check(value: Settings): (FieldError | null)[] {
  const out: (FieldError | null)[] = [];
  if (value.levelup_message !== null) {
    const c = checkMessage("levelup", value.levelup_message);
    out.push(c.ok ? null : c);
  }
  const voice = checkVoiceAmount(value.voice_xp_amount);
  out.push(voice.ok ? null : voice);
  return out;
}

export function Leveling() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const discord = useDiscord(guild);
  const form = useSettingsForm(guild, data.settings, check);
  const { value, set, errors } = form;

  return (
    <div className="settings-page">
      <PageHead
        title="LEVELING"
        lead="XP for chatting and voice, where level-ups go, multipliers, and the roles levels earn."
      />

      <Panel title="CHAT XP">
        <SwitchRow
          title="Leveling"
          control={
            <PixelToggle
              label="Leveling"
              checked={value.leveling_enabled}
              onChange={(on) => set("leveling_enabled", on)}
            />
          }
        >
          Members earn XP for chatting, climb levels and get a level-up message. Levels already earned stay
          when it is off.
        </SwitchRow>
        <Field label="Where level-ups go" error={errors.levelup_mode}>
          <Choice
            name="Where level-ups go"
            value={value.levelup_mode}
            options={MODES}
            onChange={(mode) => set("levelup_mode", mode)}
          />
        </Field>
        <div className="form-grid">
          {value.levelup_mode === "channel" && (
            <Field
              label="Level-up channel"
              htmlFor="levelup-channel"
              error={errors.levelup_channel_id}
              hint="Empty posts where the member chatted."
            >
              <ChannelPicker
                id="levelup-channel"
                info={discord.data}
                value={value.levelup_channel_id}
                onChange={(id) => set("levelup_channel_id", id)}
                none="Where they chatted"
                invalid={!!errors.levelup_channel_id}
              />
            </Field>
          )}
          <MessageField
            guild={guild}
            kind="levelup"
            label="Level-up message"
            value={value.levelup_message}
            onChange={(text) => set("levelup_message", text)}
            error={errors.levelup_message}
          />
        </div>
      </Panel>

      <Panel title="VOICE XP">
        <SwitchRow
          title="Voice XP"
          control={
            <PixelToggle
              label="Voice XP"
              checked={value.voice_xp_enabled}
              onChange={(on) => set("voice_xp_enabled", on)}
            />
          }
        >
          Members in a voice channel with someone else earn XP every minute; not while muted and deafened, and
          not in the AFK channel.
        </SwitchRow>
        <Field
          label="XP a minute"
          htmlFor="voice-amount"
          error={errors.voice_xp_amount}
          hint={`${LIMITS.voice_xp[0]} to ${LIMITS.voice_xp[1]}. Chat gives 15 to 25 a minute.`}
        >
          <input
            id="voice-amount"
            className="pixel-input input-short"
            type="number"
            min={LIMITS.voice_xp[0]}
            max={LIMITS.voice_xp[1]}
            value={value.voice_xp_amount}
            onChange={(event) => set("voice_xp_amount", Number(event.target.value))}
            aria-invalid={errors.voice_xp_amount ? true : undefined}
          />
        </Field>
        {value.voice_xp_enabled && !value.leveling_enabled && (
          <p className="notice">Leveling is off, so nobody earns XP. Turn it on above.</p>
        )}
      </Panel>

      <Multipliers guild={guild} info={discord.data} />

      <LevelRoles
        guild={guild}
        info={discord.data}
        stack={value.level_roles_stack}
        setStack={(s) => set("level_roles_stack", s)}
      />

      <RepRoles guild={guild} info={discord.data} />
      <XpAdmin guild={guild} />

      <SaveBar
        count={form.changes}
        saving={form.saving}
        blocked={form.blocked}
        error={form.barError}
        onSave={form.submit}
        onReset={form.reset}
      />
      <Toast message={form.toast} onDone={form.clearToast} />
    </div>
  );
}

/** The message of a failed list write, for the add form. */
function writeError(error: unknown): string | null {
  return error instanceof Error ? error.message : null;
}

function Multipliers({ guild, info }: { guild: string; info: GuildInfo | undefined }) {
  const { query, write } = useXpRules(guild);
  const [kind, setKind] = useState<"channel" | "role">("channel");
  const [target, setTarget] = useState<string | null>(null);
  const [multiplier, setMultiplier] = useState("2");
  const [confirmClear, setConfirmClear] = useState(false);
  const local = multiplier === "" ? "Write a multiplier." : problem(checkMultiplier(Number(multiplier)));
  const items = query.data?.items ?? [];
  const add = (value: number) =>
    target &&
    write.mutate(
      { method: "PUT", sub: `/${kind}/${target}`, body: { multiplier: value } },
      { onSuccess: () => setTarget(null) },
    );

  return (
    <Panel
      title="XP MULTIPLIERS"
      aside={
        <span className="pixel-label muted">{`${items.length}/${query.data?.max ?? LIMITS.max_xp_rules}`}</span>
      }
    >
      <p className="muted panel-lead">
        Scale the XP and points a channel or role earns: 2 doubles it, 0 earns none. A channel&apos;s
        multiplier times the member&apos;s best role multiplier; a no-XP role always wins. Server XP only;
        global XP stays normal.
      </p>
      {items.length === 0 ? (
        <p className="empty-row muted">No multipliers yet: everything earns normal XP.</p>
      ) : (
        <ul className="list" aria-label="XP multipliers">
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
                {rule.multiplier === 0 ? "NO XP" : `×${rule.multiplier}`}
              </span>
              <PixelButton
                variant="ghost"
                className="button-small"
                aria-label="Remove multiplier"
                disabled={write.isPending}
                onClick={() => write.mutate({ method: "DELETE", sub: `/${rule.kind}/${rule.target_id}` })}
              >
                Remove
              </PixelButton>
            </li>
          ))}
        </ul>
      )}
      <div className="add-row add-row-rule">
        <Field label="For" htmlFor="rule-kind">
          <select
            id="rule-kind"
            className="pixel-input pixel-select"
            value={kind}
            onChange={(event) => {
              setKind(event.target.value as "channel" | "role");
              setTarget(null);
            }}
          >
            <option value="channel">A channel or category</option>
            <option value="role">A role</option>
          </select>
        </Field>
        <Field label={kind === "channel" ? "Channel" : "Role"} htmlFor="rule-target">
          {kind === "channel" ? (
            <ChannelPicker id="rule-target" info={info} value={target} onChange={setTarget} any />
          ) : (
            <RolePicker id="rule-target" info={info} value={target} onChange={setTarget} everyone />
          )}
        </Field>
        <Field label="Multiplier" htmlFor="rule-value" error={local}>
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
            Set
          </PixelButton>
          <PixelButton variant="secondary" disabled={!target || write.isPending} onClick={() => add(0)}>
            No XP
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
              <span>Remove all {items.length} multipliers?</span>
              <PixelButton variant="ghost" onClick={() => setConfirmClear(false)}>
                Keep them
              </PixelButton>
              <PixelButton
                onClick={() =>
                  write.mutate({ method: "DELETE", sub: "" }, { onSettled: () => setConfirmClear(false) })
                }
              >
                Remove all
              </PixelButton>
            </>
          ) : (
            <PixelButton variant="ghost" onClick={() => setConfirmClear(true)}>
              Clear all
            </PixelButton>
          )}
        </div>
      )}
    </Panel>
  );
}

function LevelRoles({
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
  const local = level === "" ? "Write a level." : problem(checkLevel(Number(level)));
  const items = query.data?.items ?? [];
  const serverField = write.error instanceof ApiError ? write.error.field : undefined;

  return (
    <Panel
      title="LEVEL ROLES"
      aside={
        <span className="pixel-label muted">{`${items.length}/${query.data?.max ?? LIMITS.max_level_roles}`}</span>
      }
    >
      <Field label="When a member earns a new one">
        <Choice
          name="Level role mode"
          value={stack ? "stack" : "replace"}
          options={[
            { value: "stack", title: "Stack", detail: "Keep every role earned." },
            { value: "replace", title: "Replace", detail: "Keep only the highest one." },
          ]}
          onChange={(mode) => setStack(mode === "stack")}
        />
      </Field>
      {items.length === 0 ? (
        <p className="empty-row muted">No level roles yet.</p>
      ) : (
        <ul className="list" aria-label="Level roles">
          {items.map((row) => (
            <li key={row.role_id} className="list-row">
              <span className="list-level pixel-label">LV {row.level}</span>
              <span className="list-main">
                <RoleTag info={info} id={row.role_id} />
              </span>
              <PixelButton
                variant="ghost"
                className="button-small"
                aria-label="Remove level role"
                disabled={write.isPending}
                onClick={() => write.mutate({ method: "DELETE", sub: `/${row.role_id}` })}
              >
                Remove
              </PixelButton>
            </li>
          ))}
        </ul>
      )}
      <div className="add-row add-row-role">
        <Field
          label="Level"
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
          label="Role"
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
                { onSuccess: () => setRole(null) },
              )
            }
          >
            Add
          </PixelButton>
        </div>
      </div>
      {write.isError && !serverField && (
        <p className="field-error" role="alert">
          <RichText text={writeError(write.error) ?? ""} />
        </p>
      )}
      <SyncMembers guild={guild} disabled={items.length === 0} />
    </Panel>
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
        <h3 className="setting-name">Sync members</h3>
        <p className="muted">
          Give members who are already past a level the roles they earned. Checks up to 1,000 members with the
          newest XP first.
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
        {running ? "Syncing…" : "Sync members"}
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

function RepRoles({ guild, info }: { guild: string; info: GuildInfo | undefined }) {
  const { query, write } = useRepRoles(guild);
  const [reputation, setReputation] = useState("10");
  const [role, setRole] = useState<string | null>(null);
  const picked = info?.roles.find((r) => r.id === role);
  const local = reputation === "" ? "Write a count." : problem(checkReputation(Number(reputation)));
  const items = query.data?.items ?? [];
  return (
    <Panel
      title="REPUTATION ROLES"
      aside={
        <span className="pixel-label muted">{`${items.length}/${query.data?.max ?? LIMITS.max_rep_roles}`}</span>
      }
    >
      <p className="muted panel-lead">
        Roles members get the moment their reputation reaches a count. They keep every one.
      </p>
      {items.length === 0 ? (
        <p className="empty-row muted">No reputation roles yet.</p>
      ) : (
        <ul className="list" aria-label="Reputation roles">
          {items.map((row) => (
            <li key={row.role_id} className="list-row">
              <span className="list-level pixel-label">{row.reputation} REP</span>
              <span className="list-main">
                <RoleTag info={info} id={row.role_id} />
              </span>
              <PixelButton
                variant="ghost"
                className="button-small"
                aria-label="Remove reputation role"
                disabled={write.isPending}
                onClick={() => write.mutate({ method: "DELETE", sub: `/${row.role_id}` })}
              >
                Remove
              </PixelButton>
            </li>
          ))}
        </ul>
      )}
      <div className="add-row add-row-role">
        <Field label="Reputation" htmlFor="rep-role-count" error={local}>
          <input
            id="rep-role-count"
            className="pixel-input input-short"
            type="number"
            min={LIMITS.reputation[0]}
            max={LIMITS.reputation[1]}
            value={reputation}
            onChange={(event) => setReputation(event.target.value)}
          />
        </Field>
        <Field
          label="Role"
          htmlFor="rep-role-role"
          error={picked && !picked.assignable ? picked.why_not : null}
        >
          <RolePicker id="rep-role-role" info={info} value={role} onChange={setRole} assignable />
        </Field>
        <div className="add-actions">
          <PixelButton
            disabled={!role || !!local || (picked && !picked.assignable) || write.isPending}
            onClick={() =>
              role &&
              write.mutate(
                { method: "PUT", sub: `/${role}`, body: { reputation: Number(reputation) } },
                { onSuccess: () => setRole(null) },
              )
            }
          >
            Add
          </PixelButton>
        </div>
      </div>
      {write.isError && (
        <p className="field-error" role="alert">
          <RichText text={writeError(write.error) ?? ""} />
        </p>
      )}
    </Panel>
  );
}
