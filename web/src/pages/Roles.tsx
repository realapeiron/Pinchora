import { useState } from "react";
import { useOutletContext } from "react-router";

import { Choice } from "../components/Choice";
import { DurationInput } from "../components/DurationInput";
import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { RolePicker, RoleTag } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { RichText } from "../components/RichText";
import { Toast } from "../components/Toast";
import { ApiError, type GuildInfo, type Overview } from "../lib/api";
import { useDiscord } from "../lib/queries";
import {
  type AutoKind,
  type AutoRole,
  type RolesView,
  problemWithDelay,
  problemWithExpiry,
  spanText,
  useRoles,
  useRolesWrite,
} from "../lib/roles";
import "../styles/roles.css";
import { ColorRoles } from "./ColorRoles";
import { PageHead } from "./PageHead";
import { RolePanels } from "./RolePanels";
import { Loading, Problem } from "./States";

const KINDS: { value: AutoKind; title: string; detail: string }[] = [
  { value: "join", title: "Members who join", detail: "People who come into the server." },
  { value: "bot", title: "Bots that join", detail: "Bots that get added to the server." },
  { value: "after_verify", title: "After the join gate", detail: "Members once they are let in." },
];

const kindName = (kind: AutoKind) => KINDS.find((k) => k.value === kind)?.title ?? kind;

export function Roles() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const discord = useDiscord(guild);
  const roles = useRoles(guild);
  if (roles.isPending) return <Loading label="Loading roles" />;
  if (roles.isError) return <Problem error={roles.error} />;
  const view = roles.data;
  return (
    <div className="settings-page">
      <PageHead
        title="ROLES"
        lead="Roles members get on their own, panels where they pick their own, and colours for their names."
      />
      {discord.data && !discord.data.bot.manage_roles && (
        <p className="notice notice-heart">
          <RichText text="The bot lacks **Manage Roles**, so it can't hand out any role yet." />
        </p>
      )}
      <AutoRoles guild={guild} info={discord.data} view={view} />
      <RolePanels guild={guild} info={discord.data} view={view} />
      <ColorRoles guild={guild} info={discord.data} view={view} />
    </div>
  );
}

/** The sentence a failed write ends in, for the field it names. */
function problemOf(error: unknown, field: string): string | null {
  return error instanceof ApiError && error.errors.some((e) => e.field === field)
    ? (error.errors.find((e) => e.field === field)?.message ?? null)
    : null;
}

function AutoRoles({ guild, info, view }: { guild: string; info: GuildInfo | undefined; view: RolesView }) {
  const write = useRolesWrite(guild);
  const [role, setRole] = useState<string | null>(null);
  const [kind, setKind] = useState<AutoKind>("join");
  const [delay, setDelay] = useState(0);
  const [expiry, setExpiry] = useState(0);
  const [fresh, setFresh] = useState(0);
  const [toast, setToast] = useState<string | null>(null);
  const picked = info?.roles.find((r) => r.id === role);
  const { items, max, sticky } = view.auto;
  const delayProblem = problemWithDelay(delay);
  const expiryProblem = problemWithExpiry(expiry);
  const refused = picked && !picked.assignable ? picked.why_not : null;
  const roleName = (id: string) => info?.roles.find((r) => r.id === id)?.name ?? "a deleted role";

  const save = (
    row: Pick<AutoRole, "role_id" | "kind" | "delay_secs" | "remove_after_secs" | "enabled">,
    done?: () => void,
  ) =>
    write.mutate(
      {
        method: "PUT",
        sub: `/auto/${row.role_id}/${row.kind}`,
        body: { delay_secs: row.delay_secs, remove_after_secs: row.remove_after_secs, enabled: row.enabled },
      },
      { onSuccess: done },
    );

  return (
    <Panel title="AUTO ROLES" aside={<span className="pixel-label muted">{`${items.length}/${max}`}</span>}>
      <p className="muted panel-lead">
        Give a role to everyone who joins, with a delay if you like, and take it back after a while. Adding
        the same role and kind again changes it.
      </p>
      {items.length === 0 ? (
        <p className="empty-row muted">No auto roles yet.</p>
      ) : (
        <ul className="list" aria-label="Auto roles">
          {items.map((row) => (
            <li
              key={`${row.role_id}:${row.kind}`}
              className={`list-row auto-row ${row.enabled ? "" : "is-paused"}`}
            >
              <span className="list-main">
                <RoleTag info={info} id={row.role_id} />
                <span className="auto-meta">
                  <span className="badge badge-ink">{kindName(row.kind).toUpperCase()}</span>
                  {row.delay_secs > 0 && <span className="muted">after {spanText(row.delay_secs)}</span>}
                  {row.remove_after_secs > 0 && (
                    <span className="muted">taken back after {spanText(row.remove_after_secs)}</span>
                  )}
                  {!row.enabled && <span className="badge badge-heart">PAUSED</span>}
                </span>
              </span>
              <PixelToggle
                label={`Give ${roleName(row.role_id)} to ${kindName(row.kind).toLowerCase()}`}
                checked={row.enabled}
                disabled={write.isPending}
                onChange={(on) =>
                  save({
                    role_id: row.role_id,
                    kind: row.kind,
                    delay_secs: row.delay_secs,
                    remove_after_secs: row.remove_after_secs,
                    enabled: on,
                  })
                }
              />
              <PixelButton
                variant="ghost"
                className="button-small"
                aria-label={`Remove ${roleName(row.role_id)} from ${kindName(row.kind).toLowerCase()}`}
                disabled={write.isPending}
                onClick={() => write.mutate({ method: "DELETE", sub: `/auto/${row.role_id}/${row.kind}` })}
              >
                Remove
              </PixelButton>
            </li>
          ))}
        </ul>
      )}

      <div className="auto-form">
        <Field label="Who gets it">
          <Choice name="Who gets the role" value={kind} options={KINDS} onChange={setKind} />
        </Field>
        <div className="auto-fields">
          <Field
            label="Role"
            htmlFor="auto-role"
            error={refused ?? problemOf(write.error, "role_id")}
            hint="The bot's role must sit above it."
          >
            <RolePicker id="auto-role" info={info} value={role} onChange={setRole} assignable />
          </Field>
          <Field
            label="Wait before giving it"
            htmlFor="auto-delay"
            error={delayProblem ?? problemOf(write.error, "delay_secs")}
            hint="0 gives it at once."
          >
            <DurationInput
              key={`d${fresh}`}
              id="auto-delay"
              label="Wait before giving it"
              value={delay}
              onChange={setDelay}
              invalid={!!delayProblem}
            />
          </Field>
          <Field
            label="Take it back after"
            htmlFor="auto-expiry"
            error={expiryProblem ?? problemOf(write.error, "remove_after_secs")}
            hint="0 keeps it for good."
          >
            <DurationInput
              key={`e${fresh}`}
              id="auto-expiry"
              label="Take it back after"
              value={expiry}
              onChange={setExpiry}
              invalid={!!expiryProblem}
            />
          </Field>
          <div className="add-actions">
            <PixelButton
              disabled={!role || !!refused || !!delayProblem || !!expiryProblem || write.isPending}
              onClick={() =>
                role &&
                save(
                  { role_id: role, kind, delay_secs: delay, remove_after_secs: expiry, enabled: true },
                  () => {
                    setRole(null);
                    setDelay(0);
                    setExpiry(0);
                    setFresh((n) => n + 1);
                    setToast("Auto role saved.");
                  },
                )
              }
            >
              {items.some((r) => r.role_id === role && r.kind === kind) ? "Update" : "Add"}
            </PixelButton>
          </div>
        </div>
      </div>
      {write.isError &&
        !problemOf(write.error, "role_id") &&
        !problemOf(write.error, "delay_secs") &&
        !problemOf(write.error, "remove_after_secs") && (
          <p className="field-error" role="alert">
            <RichText text={write.error instanceof Error ? write.error.message : ""} />
          </p>
        )}

      <h3 className="setting-name" style={{ marginTop: 24 }}>
        What happens when someone joins
      </h3>
      <Timeline items={items} info={info} />

      <SwitchRow
        title="Sticky roles"
        control={
          <PixelToggle
            label="Sticky roles"
            checked={sticky}
            disabled={write.isPending}
            onChange={(on) => write.mutate({ method: "PUT", sub: "/sticky", body: { enabled: on } })}
          />
        }
      >
        A member who leaves and comes back within 60 days gets their roles back, as far as the bot can give
        them (not roles above its own, roles with admin powers, or roles an integration owns). Turning it on
        starts from the roles members hold now; turning it off forgets what was kept.
      </SwitchRow>
      <Toast message={toast} onDone={() => setToast(null)} />
    </Panel>
  );
}

/** The auto roles in the order a member meets them: what they get at once, and what comes later. */
function Timeline({ items, info }: { items: AutoRole[]; info: GuildInfo | undefined }) {
  const kinds = KINDS.filter((k) => items.some((r) => r.kind === k.value && r.enabled));
  if (kinds.length === 0) return <p className="empty-row muted">Nobody gets a role on their own yet.</p>;
  return (
    <div className="stack">
      {kinds.map((k) => (
        <section key={k.value} aria-label={k.title}>
          <p className="pixel-label muted">{k.title.toUpperCase()}</p>
          <ol className="timeline">
            {items
              .filter((r) => r.kind === k.value && r.enabled)
              .sort((a, b) => a.delay_secs - b.delay_secs)
              .map((r) => (
                <li key={r.role_id}>
                  <span className="timeline-when pixel-label">
                    {r.delay_secs > 0 ? `+ ${spanText(r.delay_secs)}` : "AT ONCE"}
                  </span>
                  <RoleTag info={info} id={r.role_id} />
                  {r.remove_after_secs > 0 && (
                    <span className="muted list-note">taken back {spanText(r.remove_after_secs)} later</span>
                  )}
                </li>
              ))}
          </ol>
        </section>
      ))}
    </div>
  );
}
