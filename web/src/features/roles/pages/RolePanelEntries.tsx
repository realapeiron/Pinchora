import { translate as spaTranslate } from "../../../i18n";
import { DurationInput } from "../components/DurationInput";
import { Field } from "../../../components/Field";
import { RolePicker } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { RichText } from "../../../components/RichText";
import type { GuildInfo } from "../../../lib/api";
import { LIMITS, type PanelDraft, newKey } from "../api/roles";

export function RolePanelEntries({
  draft,
  saved,
  info,
  why,
  warnings,
  setRoles,
  setRow,
  move,
}: {
  draft: PanelDraft;
  saved: PanelDraft;
  info: GuildInfo | undefined;
  why: (field: string) => string | null;
  warnings: string[];
  setRoles: (roles: PanelDraft["roles"]) => void;
  setRow: (key: string, patch: Partial<PanelDraft["roles"][number]>) => void;
  move: (index: number, by: -1 | 1) => void;
}) {
  return (
    <div>
      <div className="editor-title">
        <h3 className="setting-name">{spaTranslate("Roles")}</h3>
        <PixelButton
          variant="secondary"
          className="button-small"
          disabled={draft.roles.length >= LIMITS.entries}
          onClick={() =>
            setRoles([
              ...draft.roles,
              {
                key: newKey(),
                role_id: "",
                label: "",
                emoji: null,
                description: null,
                temp_secs: null,
              },
            ])
          }
        >
          {" "}
          {spaTranslate("Add a role")}{" "}
        </PixelButton>
      </div>
      {draft.roles.length === 0 && (
        <p className="empty-row muted">{spaTranslate("No roles on this panel yet.")}</p>
      )}
      <ol className="entry-list" aria-label={spaTranslate("Roles on the panel")}>
        {draft.roles.map((row, index) => {
          const picked = info?.roles.find((r) => r.id === row.role_id);
          const kept = saved.roles.some((r) => r.role_id === row.role_id);
          const refused = picked && !picked.assignable && !kept ? picked.why_not : null;
          const id = `entry-${row.key}`;
          return (
            <li key={row.key} className="entry">
              <div className="entry-fields">
                <div className="field-wide">
                  <Field label={spaTranslate("Role")} htmlFor={`${id}-role`} error={refused}>
                    <RolePicker
                      id={`${id}-role`}
                      info={info}
                      value={row.role_id || null}
                      assignable
                      onChange={(role) =>
                        setRow(row.key, {
                          role_id: role ?? "",
                          label: row.label || (info?.roles.find((r) => r.id === role)?.name ?? ""),
                        })
                      }
                    />
                  </Field>
                </div>
                <Field label={spaTranslate("Label")} htmlFor={`${id}-label`}>
                  <input
                    id={`${id}-label`}
                    className="pixel-input"
                    value={row.label}
                    maxLength={LIMITS.label + 20}
                    onChange={(event) => setRow(row.key, { label: event.target.value })}
                  />
                </Field>
                <Field
                  label={spaTranslate("Emoji")}
                  htmlFor={`${id}-emoji`}
                  hint={draft.style === "reactions" ? spaTranslate("Needed: it is the reaction.") : undefined}
                >
                  <input
                    id={`${id}-emoji`}
                    className="pixel-input"
                    value={row.emoji ?? ""}
                    maxLength={64}
                    placeholder={spaTranslate("🎮 or <:name:id>")}
                    onChange={(event) => setRow(row.key, { emoji: event.target.value || null })}
                  />
                </Field>
                <div>
                  <Field
                    label={spaTranslate("Line under the label")}
                    htmlFor={`${id}-line`}
                    hint={draft.style === "select" ? spaTranslate("Shown in the menu.") : undefined}
                  >
                    <input
                      id={`${id}-line`}
                      className="pixel-input"
                      value={row.description ?? ""}
                      maxLength={LIMITS.line}
                      onChange={(event) => setRow(row.key, { description: event.target.value || null })}
                    />
                  </Field>
                </div>
                <Field
                  label={spaTranslate("Lasts")}
                  htmlFor={`${id}-lasts`}
                  hint={spaTranslate("After a member takes it; 0 keeps it.")}
                >
                  <DurationInput
                    id={`${id}-lasts`}
                    label={`${row.label || "Role"} lasts`}
                    value={row.temp_secs ?? 0}
                    onChange={(secs) => setRow(row.key, { temp_secs: secs > 0 ? secs : null })}
                  />
                </Field>
              </div>
              <div className="entry-tools">
                <PixelButton
                  variant="secondary"
                  aria-label={spaTranslate("Move {arg1} up", { arg1: row.label || "this role" })}
                  disabled={index === 0}
                  onClick={() => move(index, -1)}
                >
                  ↑
                </PixelButton>
                <PixelButton
                  variant="secondary"
                  aria-label={spaTranslate("Move {arg1} down", { arg1: row.label || "this role" })}
                  disabled={index === draft.roles.length - 1}
                  onClick={() => move(index, 1)}
                >
                  ↓
                </PixelButton>
                <PixelButton
                  variant="ghost"
                  aria-label={spaTranslate("Remove {arg1}", { arg1: row.label || "this role" })}
                  onClick={() => setRoles(draft.roles.filter((r) => r.key !== row.key))}
                >
                  ✕
                </PixelButton>
              </div>
            </li>
          );
        })}
      </ol>
      {(why("roles") ?? null) && (
        <p className="field-error" role="alert">
          <RichText text={why("roles") ?? ""} />
        </p>
      )}
      {warnings.length > 0 && (
        <p className="notice">
          {" "}
          {spaTranslate("A reactions panel needs its own emoji on every role:")} {warnings.join(" ")}
        </p>
      )}
    </div>
  );
}
