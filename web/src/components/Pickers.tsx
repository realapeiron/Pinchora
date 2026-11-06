import type { Channel, GuildInfo, Role } from "../lib/api";

/** Channel types the bot posts into: text and announcement. */
const POSTABLE = new Set([0, 5]);
const CATEGORY = 4;

/** A channel as the pickers write it: `#general`, `🔊 Lounge`, `▸ Category`. */
export function channelLabel(channel: Channel): string {
  return prefix(channel) + channel.name;
}

function prefix(channel: Channel): string {
  if (channel.type === CATEGORY) return "▸ ";
  if (channel.type === 2 || channel.type === 13) return "🔊 ";
  return "#";
}

/** Channels grouped under their categories, in Discord's order. */
function grouped(channels: Channel[], keep: (c: Channel) => boolean) {
  const categories = channels.filter((c) => c.type === CATEGORY);
  const loose = channels.filter((c) => c.type !== CATEGORY && !c.parent_id && keep(c));
  const groups = categories
    .map((category) => ({
      category,
      channels: channels.filter((c) => c.parent_id === category.id && keep(c)),
    }))
    .filter((g) => g.channels.length > 0 || keep(g.category));
  return { loose, groups };
}

/**
 * A channel `<select>`: text and announcement channels by default, or every channel and category
 * (`any`) for XP multipliers. `none` is the label of the empty choice, when there is one.
 */
export function ChannelPicker({
  id,
  label,
  info,
  value,
  onChange,
  none,
  any = false,
  disabled,
  invalid,
}: {
  id?: string;
  /** The picker's name for screen readers, when no `<label>` points at it. */
  label?: string;
  info: GuildInfo | undefined;
  value: string | null;
  onChange: (id: string | null) => void;
  none?: string;
  any?: boolean;
  disabled?: boolean;
  invalid?: boolean;
}) {
  const keep = (c: Channel) => any || POSTABLE.has(c.type);
  const { loose, groups } = grouped(info?.channels ?? [], keep);
  const known = !value || (info?.channels ?? []).some((c) => c.id === value);
  return (
    <select
      id={id}
      aria-label={label}
      className="pixel-input pixel-select"
      value={value ?? ""}
      onChange={(event) => onChange(event.target.value || null)}
      disabled={disabled || !info}
      aria-invalid={invalid || undefined}
    >
      {none !== undefined && <option value="">{none}</option>}
      {none === undefined && !value && (
        <option value="" disabled>
          Pick a channel
        </option>
      )}
      {!known && <option value={value!}>Deleted channel ({value})</option>}
      {loose.map((c) => (
        <option key={c.id} value={c.id}>
          {prefix(c) + c.name}
        </option>
      ))}
      {groups.map(({ category, channels }) => (
        <optgroup key={category.id} label={category.name.toUpperCase()}>
          {any && <option value={category.id}>{`▸ ${category.name} (whole category)`}</option>}
          {channels.map((c) => (
            <option key={c.id} value={c.id}>
              {prefix(c) + c.name}
            </option>
          ))}
        </optgroup>
      ))}
    </select>
  );
}

/** A role's colour as CSS, Discord's grey for none. */
export function roleColor(role: Pick<Role, "color"> | undefined): string {
  return role && role.color ? `#${role.color.toString(16).padStart(6, "0")}` : "#99aab5";
}

/**
 * A role `<select>`. With `assignable`, roles you may not set up to be handed out (the bot cannot give
 * them, they sit at or above your highest role, or they carry powers only an administrator hands out)
 * are listed but disabled, and the sentence `/config` would answer with shows under the picker when one
 * is picked anyway. `forActions` applies a custom action's stricter rule: never a role with powers.
 */
export function RolePicker({
  id,
  info,
  value,
  onChange,
  none,
  assignable = false,
  forActions = false,
  everyone = false,
  disabled,
  invalid,
}: {
  id?: string;
  info: GuildInfo | undefined;
  value: string | null;
  onChange: (id: string | null) => void;
  none?: string;
  assignable?: boolean;
  forActions?: boolean;
  everyone?: boolean;
  disabled?: boolean;
  invalid?: boolean;
}) {
  const refused = (r: Role) => assignable && !(forActions ? r.action_ok : r.assignable);
  const roles = (info?.roles ?? []).filter((r) => everyone || !r.everyone);
  const known = !value || roles.some((r) => r.id === value);
  return (
    <select
      id={id}
      className="pixel-input pixel-select"
      value={value ?? ""}
      onChange={(event) => onChange(event.target.value || null)}
      disabled={disabled || !info}
      aria-invalid={invalid || undefined}
    >
      {none !== undefined && <option value="">{none}</option>}
      {none === undefined && !value && (
        <option value="" disabled>
          Pick a role
        </option>
      )}
      {!known && <option value={value!}>Deleted role ({value})</option>}
      {roles.map((r) => (
        <option key={r.id} value={r.id} disabled={refused(r)}>
          {(r.everyone ? "@everyone" : `@${r.name}`) + (refused(r) ? why(r, info, forActions) : "")}
        </option>
      ))}
    </select>
  );
}

/** A short reason for a disabled option; the full sentence shows under the picker. */
export function why(role: Role, info: GuildInfo | undefined, forActions = false): string {
  if (role.everyone) return " — everyone has it";
  if (role.managed) return " — managed by an integration";
  // Discord would not say where the bot's own role sits: it may not be above anything.
  const bot = info?.bot.top_role_position;
  if (bot === null) return " — can't check my role yet";
  if (bot !== undefined && role.position >= bot) return " — above my role";
  const you = info?.you;
  if (role.powers.length > 0 && (forActions || !(you?.owner || you?.administrator)))
    return " — has admin powers";
  if (you && !you.owner) {
    if (you.top_role_position === null) return " — can't check your roles yet";
    if (role.position >= you.top_role_position) return " — not below your top role";
  }
  return " — not allowed";
}

/** A role name in its colour, for lists. */
export function RoleTag({ info, id }: { info: GuildInfo | undefined; id: string }) {
  const role = info?.roles.find((r) => r.id === id);
  const name = role ? (role.everyone ? "@everyone" : `@${role.name}`) : `Deleted role`;
  return (
    <span className="role-tag" title={id}>
      <span className="role-dot" style={{ background: roleColor(role) }} aria-hidden="true" />
      {name}
    </span>
  );
}

export function ChannelTag({ info, id }: { info: GuildInfo | undefined; id: string }) {
  const channel = info?.channels.find((c) => c.id === id);
  return (
    <span className="channel-tag" title={id}>
      {channel ? prefix(channel) + channel.name : "Deleted channel"}
    </span>
  );
}
