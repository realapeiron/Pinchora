import type { GuildInfo, TagAction, TriggerKind } from "../lib/api";
import { LIMITS, actionNeedsRole } from "../lib/tags";
import { Field } from "./Field";
import { RolePicker, why } from "./Pickers";
import { PixelButton } from "./PixelButton";

const KINDS: { value: TagAction["kind"]; label: string; keywordOnly?: boolean }[] = [
  { value: "add_role", label: "Give a role" },
  { value: "remove_role", label: "Take a role" },
  { value: "toggle_role", label: "Give it, or take it if they have it" },
  { value: "give_points", label: "Give or take server points" },
  { value: "dm", label: "Send a direct message" },
  { value: "react", label: "React to the message", keywordOnly: true },
];

function fresh(kind: TagAction["kind"]): TagAction {
  switch (kind) {
    case "give_points":
      return { kind, amount: 10 };
    case "dm":
      return { kind, text: "" };
    case "react":
      return { kind, emoji: "🎉" };
    default:
      return { kind, role_id: "" };
  }
}

/** Up to three things done besides answering. Roles that may not be handed out are greyed out with the reason. */
export function ActionsEditor({
  actions,
  onChange,
  info,
  trigger,
  error,
}: {
  actions: TagAction[];
  onChange: (next: TagAction[]) => void;
  info: GuildInfo | undefined;
  /** The responder's trigger; a command has none. Only a keyword has a message to react to. */
  trigger?: TriggerKind;
  error?: string | null;
}) {
  const kinds = KINDS.filter((k) => !k.keywordOnly || trigger === "keyword");
  const set = (index: number, next: TagAction) => onChange(actions.map((a, i) => (i === index ? next : a)));
  return (
    <Field
      label="Actions"
      error={error}
      hint={`Done right after the answer, for the member it is about. With actions, a member waits at least ${LIMITS.action_cooldown_min} s between uses; points from actions stop at ${LIMITS.action_points_daily_max.toLocaleString("en-US")} a member a day, direct messages at ${LIMITS.action_dms_per_hour} an hour, and roles with admin powers are never handed out.`}
    >
      {actions.length === 0 && <p className="muted">Nothing besides the answer.</p>}
      <ul className="actions-list">
        {actions.map((action, index) => {
          const role = actionNeedsRole(action) ? info?.roles.find((r) => r.id === action.role_id) : undefined;
          return (
            <li key={index} className="action-row">
              <select
                className="pixel-input pixel-select"
                aria-label={`Action ${index + 1}`}
                value={action.kind}
                onChange={(event) => set(index, fresh(event.target.value as TagAction["kind"]))}
              >
                {kinds.map((k) => (
                  <option key={k.value} value={k.value}>
                    {k.label}
                  </option>
                ))}
              </select>
              {actionNeedsRole(action) && (
                <div className="action-value">
                  <RolePicker
                    info={info}
                    value={action.role_id || null}
                    onChange={(id) => set(index, { ...action, role_id: id ?? "" })}
                    assignable
                    forActions
                    none="Pick a role"
                  />
                  {role && !role.action_ok && (
                    <p className="field-error" role="alert">
                      {role.action_why_not ?? why(role, info, true)}
                    </p>
                  )}
                </div>
              )}
              {action.kind === "give_points" && (
                <input
                  type="number"
                  className="pixel-input input-short"
                  aria-label="Points, negative to take"
                  min={LIMITS.points_action[0]}
                  max={LIMITS.points_action[1]}
                  value={action.amount}
                  onChange={(event) => set(index, { ...action, amount: Number(event.target.value) })}
                />
              )}
              {action.kind === "dm" && (
                <textarea
                  className="pixel-input pixel-textarea action-value"
                  aria-label="Direct message text"
                  rows={2}
                  maxLength={LIMITS.response_max}
                  value={action.text}
                  onChange={(event) => set(index, { ...action, text: event.target.value })}
                />
              )}
              {action.kind === "react" && (
                <input
                  className="pixel-input input-short"
                  aria-label="Emoji"
                  value={action.emoji}
                  onChange={(event) => set(index, { ...action, emoji: event.target.value })}
                />
              )}
              <PixelButton
                variant="ghost"
                className="button-small"
                aria-label={`Remove action ${index + 1}`}
                onClick={() => onChange(actions.filter((_, i) => i !== index))}
              >
                Remove
              </PixelButton>
            </li>
          );
        })}
      </ul>
      {actions.length < LIMITS.max_actions && (
        <div>
          <PixelButton
            variant="secondary"
            className="button-small"
            onClick={() => onChange([...actions, fresh("add_role")])}
          >
            Add an action ({actions.length}/{LIMITS.max_actions})
          </PixelButton>
        </div>
      )}
    </Field>
  );
}
