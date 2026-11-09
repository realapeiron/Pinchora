import { plural as spaPlural, translate as spaTranslate } from "../../../i18n";
import { useState } from "react";

import { Choice } from "../../../components/Choice";
import { DurationInput } from "../components/DurationInput";
import { Field, SwitchRow } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { ChannelPicker, ChannelTag, RolePicker, roleColor } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { PixelToggle } from "../../../components/PixelToggle";
import { RichText } from "../../../components/RichText";
import { SaveBar } from "../../../components/SaveBar";
import { Toast } from "../../../components/Toast";
import { ApiError, type GuildInfo } from "../../../lib/api";
import {
  LIMITS,
  type Panel as PanelData,
  type PanelDraft,
  STYLES,
  type RolesView,
  bodyOf,
  customEmojiUrl,
  draftOf,
  editsOf,
  emptyDraft,
  footerOf,
  newKey,
  problemWithDraft,
  problemWithRules,
  reactionProblems,
  spanText,
  useRolesWrite,
} from "../api/roles";

type Editing = string | "new" | null;

export function RolePanels({
  guild,
  info,
  view,
}: {
  guild: string;
  info: GuildInfo | undefined;
  view: RolesView;
}) {
  const write = useRolesWrite(guild);
  const panels = view.panels.items;
  const [editing, setEditing] = useState<Editing>(null);
  const [saved, setSaved] = useState<PanelDraft>(emptyDraft());
  const [draft, setDraft] = useState<PanelDraft>(emptyDraft());
  const [toast, setToast] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const edits = editing ? editsOf(saved, draft) : 0;
  const local = editing ? problemWithDraft(draft) : null;
  const serverProblem = write.error instanceof ApiError ? write.error : null;

  const open = (panel: PanelData | null) => {
    write.reset();
    const next = panel ? draftOf(panel) : emptyDraft();
    setSaved(next);
    setDraft(panel ? draftOf(panel) : emptyDraft());
    setEditing(panel ? panel.id : "new");
  };

  const save = () =>
    write.mutate(
      {
        method: editing === "new" ? "POST" : "PUT",
        sub: editing === "new" ? "/panels" : `/panels/${editing}`,
        body: bodyOf(draft),
      },
      {
        onSuccess: (answer) => {
          const title = draft.title.trim().toLowerCase();
          const panel =
            answer.panels.items.find((p) => p.id === editing) ??
            answer.panels.items.find((p) => p.title.toLowerCase() === title);
          if (panel) {
            setSaved(draftOf(panel));
            setDraft(draftOf(panel));
            setEditing(panel.id);
          }
          setToast(spaTranslate("Panel saved."));
        },
      },
    );

  const current = panels.find((p) => p.id === editing) ?? null;

  return (
    <>
      <Panel
        title={spaTranslate("ROLE PANELS")}
        aside={
          <span className="pixel-label muted">
            {spaPlural(panels.length, { one: "{count} panel", other: "{count} panels" })}
          </span>
        }
      >
        <p className="muted panel-lead">
          {" "}
          {spaTranslate(
            "A message where members pick their own roles: with buttons, a menu or reactions. The bot posts it, and edits the same message when you change the panel.",
          )}{" "}
        </p>
        {panels.length === 0 ? (
          <p className="empty-row muted">{spaTranslate("No panels yet.")}</p>
        ) : (
          <ul className="panel-cards" aria-label={spaTranslate("Role panels")}>
            {panels.map((panel) => (
              <li key={panel.id} className={`panel-card ${editing === panel.id ? "is-open" : ""}`}>
                <h3 className="panel-card-title">{panel.title}</h3>
                <p className="panel-card-meta">
                  <span className="badge badge-ink">{spaTranslate(panel.style.toUpperCase())}</span>
                  <span className="muted">
                    {spaPlural(panel.roles.length, { one: "{count} role", other: "{count} roles" })}
                  </span>
                  {panel.channel_id ? (
                    <ChannelTag info={info} id={panel.channel_id} />
                  ) : (
                    <span className="badge badge-gold">{spaTranslate("DRAFT")}</span>
                  )}
                </p>
                <div className="panel-card-actions">
                  <PixelButton
                    variant="secondary"
                    className="button-small"
                    aria-label={spaTranslate("Edit {arg1}", { arg1: panel.title })}
                    onClick={() => open(panel)}
                  >
                    {spaTranslate("Edit")}
                  </PixelButton>
                  {confirmDelete === panel.id ? (
                    <>
                      <PixelButton
                        className="button-small"
                        disabled={write.isPending}
                        onClick={() =>
                          write.mutate(
                            { method: "DELETE", sub: `/panels/${panel.id}` },
                            {
                              onSettled: () => setConfirmDelete(null),
                              onSuccess: () => {
                                if (editing === panel.id) setEditing(null);
                              },
                            },
                          )
                        }
                      >
                        {" "}
                        {spaTranslate("Delete it")}{" "}
                      </PixelButton>
                      <PixelButton
                        variant="ghost"
                        className="button-small"
                        onClick={() => setConfirmDelete(null)}
                      >
                        {spaTranslate("Keep it")}
                      </PixelButton>
                    </>
                  ) : (
                    <PixelButton
                      variant="ghost"
                      className="button-small"
                      aria-label={spaTranslate("Delete {arg1}", { arg1: panel.title })}
                      onClick={() => setConfirmDelete(panel.id)}
                    >
                      {spaTranslate("Delete")}
                    </PixelButton>
                  )}
                </div>
              </li>
            ))}
          </ul>
        )}
        <div className="panel-foot">
          <PixelButton onClick={() => open(null)} disabled={editing === "new"}>
            {" "}
            {spaTranslate("New panel")}{" "}
          </PixelButton>
        </div>
      </Panel>

      {editing && (
        <Editor
          key={editing}
          info={info}
          draft={draft}
          setDraft={setDraft}
          saved={saved}
          current={current}
          unsaved={edits > 0}
          problem={local}
          server={serverProblem}
          posting={write}
        />
      )}

      <SaveBar
        count={edits}
        saving={write.isPending && !!write.variables && !write.variables.sub.endsWith("/post")}
        blocked={!!local}
        error={
          serverProblem && serverProblem.field !== "channel_id"
            ? serverProblem.message
            : local
              ? spaTranslate("Fix first: {arg1}", { arg1: local.message })
              : null
        }
        onSave={save}
        onReset={() => {
          write.reset();
          setDraft({ ...saved, roles: saved.roles.map((r) => ({ ...r, key: newKey() })) });
        }}
      />
      <Toast message={toast} onDone={() => setToast(null)} />
    </>
  );
}

/** What a failed write said about `field`, if it named it. */
function said(error: ApiError | null, field: string): string | null {
  return error?.errors.find((e) => e.field === field)?.message ?? null;
}

function Editor({
  info,
  draft,
  setDraft,
  saved,
  current,
  unsaved,
  problem,
  server,
  posting,
}: {
  info: GuildInfo | undefined;
  draft: PanelDraft;
  setDraft: (next: PanelDraft) => void;
  saved: PanelDraft;
  current: PanelData | null;
  unsaved: boolean;
  problem: { field: string; message: string } | null;
  server: ApiError | null;
  posting: ReturnType<typeof useRolesWrite>;
}) {
  const set = <K extends keyof PanelDraft>(key: K, value: PanelDraft[K]) =>
    setDraft({ ...draft, [key]: value });
  const setRow = (key: string, patch: Partial<PanelDraft["roles"][number]>) =>
    set(
      "roles",
      draft.roles.map((r) => (r.key === key ? { ...r, ...patch } : r)),
    );
  const move = (index: number, by: -1 | 1) => {
    const to = index + by;
    if (to < 0 || to >= draft.roles.length) return;
    const next = [...draft.roles];
    const [row] = next.splice(index, 1);
    if (row) next.splice(to, 0, row);
    set("roles", next);
  };
  const rules = problemWithRules(draft.exclusive, draft.min_picks, draft.max_picks);
  const number = (raw: string) => (raw === "" ? 0 : Math.max(0, Math.round(Number(raw)) || 0));
  const why = (field: string) => (problem?.field === field ? problem.message : null) ?? said(server, field);
  const warnings = reactionProblems(draft);

  return (
    <Panel
      title={current ? spaTranslate("EDIT PANEL") : spaTranslate("NEW PANEL")}
      aside={<span className="pixel-label muted">{`${draft.roles.length}/${LIMITS.entries} roles`}</span>}
    >
      <div className="panel-designer">
        <div className="designer-preview">
          <PanelPreview draft={draft} info={info} />
        </div>
        <div className="designer-controls">
          <div className="form-grid form-grid-2" style={{ marginTop: 0 }}>
            <Field
              label={spaTranslate("Title")}
              htmlFor="panel-title"
              error={why("title")}
              aside={
                <span className="counter">
                  {[...draft.title].length}/{LIMITS.title}
                </span>
              }
            >
              <input
                id="panel-title"
                className="pixel-input"
                aria-required="true"
                value={draft.title}
                maxLength={LIMITS.title + 20}
                placeholder={spaTranslate("Pick your colours")}
                onChange={(event) => set("title", event.target.value)}
                aria-invalid={why("title") ? true : undefined}
              />
            </Field>
            <Field
              label={spaTranslate("Text above the roles")}
              htmlFor="panel-description"
              error={why("description")}
              aside={
                <span
                  className={`counter ${[...draft.description].length > LIMITS.description ? "is-over" : ""}`}
                >
                  {[...draft.description].length}/{LIMITS.description}
                </span>
              }
            >
              <textarea
                id="panel-description"
                className="pixel-input pixel-textarea"
                rows={3}
                value={draft.description}
                placeholder={spaTranslate("What are these roles for?")}
                onChange={(event) => set("description", event.target.value)}
                aria-invalid={why("description") ? true : undefined}
              />
            </Field>
          </div>

          <Field label={spaTranslate("Style")}>
            <Choice
              name="Panel style"
              value={draft.style}
              options={STYLES}
              onChange={(style) => set("style", style)}
            />
          </Field>

          <div className="panel-rules">
            <Field
              label={spaTranslate("Required role")}
              htmlFor="panel-required"
              hint={spaTranslate("Only members with it can use the panel.")}
            >
              <RolePicker
                id="panel-required"
                info={info}
                value={draft.required_role}
                onChange={(role) => set("required_role", role)}
                none={spaTranslate("Anyone")}
              />
            </Field>
            <Field
              label={spaTranslate("Fewest roles")}
              htmlFor="panel-min"
              hint={spaTranslate("They keep at least this many.")}
            >
              <input
                id="panel-min"
                className="pixel-input input-short"
                type="number"
                min={0}
                max={LIMITS.entries}
                value={draft.min_picks}
                onChange={(event) => set("min_picks", number(event.target.value))}
                aria-invalid={rules || why("rules") ? true : undefined}
              />
            </Field>
            <Field
              label={spaTranslate("Most roles")}
              htmlFor="panel-max"
              hint={spaTranslate("0 has no limit.")}
            >
              <input
                id="panel-max"
                className="pixel-input input-short"
                type="number"
                min={0}
                max={LIMITS.entries}
                value={draft.max_picks}
                disabled={draft.exclusive}
                onChange={(event) => set("max_picks", number(event.target.value))}
                aria-invalid={rules || why("rules") ? true : undefined}
              />
            </Field>
          </div>
          {(rules ?? why("rules") ?? said(server, "min_picks") ?? said(server, "max_picks")) && (
            <p className="field-error" role="alert">
              {rules ?? why("rules") ?? said(server, "min_picks") ?? said(server, "max_picks")}
            </p>
          )}
          <SwitchRow
            title={spaTranslate("One at a time")}
            control={
              <PixelToggle
                label={spaTranslate("One at a time")}
                checked={draft.exclusive}
                onChange={(on) => setDraft({ ...draft, exclusive: on, max_picks: on ? 0 : draft.max_picks })}
              />
            }
          >
            {" "}
            {spaTranslate(
              "Picking a role takes the member's other role from this panel away, like a team or a colour.",
            )}{" "}
          </SwitchRow>

          <div>
            <div className="editor-title">
              <h3 className="setting-name">{spaTranslate("Roles")}</h3>
              <PixelButton
                variant="secondary"
                className="button-small"
                disabled={draft.roles.length >= LIMITS.entries}
                onClick={() =>
                  set("roles", [
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
                        hint={
                          draft.style === "reactions"
                            ? spaTranslate("Needed: it is the reaction.")
                            : undefined
                        }
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
                        onClick={() =>
                          set(
                            "roles",
                            draft.roles.filter((r) => r.key !== row.key),
                          )
                        }
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

          {current && (
            <PostRow
              info={info}
              panel={current}
              unsaved={unsaved}
              posting={posting}
              blocked={warnings.length > 0 || draft.roles.length === 0}
            />
          )}
          {!current && (
            <p className="muted fine">{spaTranslate("Save the panel first, then post it in a channel.")}</p>
          )}
        </div>
      </div>
    </Panel>
  );
}

function PostRow({
  info,
  panel,
  unsaved,
  posting,
  blocked,
}: {
  info: GuildInfo | undefined;
  panel: PanelData;
  unsaved: boolean;
  posting: ReturnType<typeof useRolesWrite>;
  blocked: boolean;
}) {
  const [channel, setChannel] = useState<string | null>(panel.channel_id);
  const error = posting.error instanceof ApiError ? posting.error : null;
  const sameSpot = !!panel.message_id && channel === panel.channel_id;
  const posted = posting.data?.posted;
  return (
    <div>
      <div className="post-row">
        <Field
          label={spaTranslate("Post it in")}
          htmlFor="panel-channel"
          error={error?.field === "channel_id" ? error.message : null}
          hint={
            unsaved
              ? spaTranslate("Save your changes first: the bot posts what is saved.")
              : sameSpot
                ? spaTranslate("The panel's message is edited in place.")
                : spaTranslate("Text or announcement channels the bot can see.")
          }
        >
          <ChannelPicker
            id="panel-channel"
            info={info}
            value={channel}
            onChange={setChannel}
            none={spaTranslate("Pick a channel")}
          />
        </Field>
        <div className="add-actions">
          <PixelButton
            disabled={!channel || unsaved || blocked || posting.isPending}
            onClick={() =>
              channel &&
              posting.mutate({
                method: "POST",
                sub: `/panels/${panel.id}/post`,
                body: { channel_id: channel },
              })
            }
          >
            {posting.isPending
              ? spaTranslate("Posting…")
              : sameSpot
                ? spaTranslate("Update message")
                : spaTranslate("Post panel")}
          </PixelButton>
        </div>
      </div>
      {error && error.field !== "channel_id" && !unsaved && posting.variables?.sub.endsWith("/post") && (
        <p className="field-error" role="alert">
          <RichText text={error.message} />
        </p>
      )}
      {posted && posting.variables?.sub === `/panels/${panel.id}/post` && (
        <p className="notice" role="status">
          {posted.updated ? spaTranslate("Updated") : spaTranslate("Posted")} {spaTranslate("in")}{" "}
          <ChannelTag info={info} id={posted.channel_id} />:{" "}
          <a href={posted.url} target="_blank" rel="noopener noreferrer">
            {" "}
            {spaTranslate("open the message")}{" "}
          </a>
          {posted.unusable.length > 0 && (
            <>
              {" "}
              {spaTranslate("Discord would not take these as reactions:")} {posted.unusable.join(" ")}.
            </>
          )}
        </p>
      )}
    </div>
  );
}

/** One emoji: the character, or the custom emoji's picture. */
function Emoji({ text }: { text: string | null }) {
  const url = customEmojiUrl(text);
  if (url) return <img src={url} alt="" />;
  return <>{text}</>;
}

/** The panel as members will see it: the bot's message with its buttons, menu or reactions. */
function PanelPreview({ draft, info }: { draft: PanelDraft; info: GuildInfo | undefined }) {
  const name = (id: string) => {
    const role = info?.roles.find((r) => r.id === id);
    return role ? `@${role.name}` : "@role";
  };
  const color = (id: string) => roleColor(info?.roles.find((r) => r.id === id));
  const rows = draft.roles.filter((r) => r.role_id);
  return (
    <div className="panel-preview" role="region" aria-label={spaTranslate("Panel preview")}>
      <div className="discord">
        <div className="discord-head">
          <span className="discord-avatar" aria-hidden="true" />
          <strong>Pinchora</strong>
          <span className="discord-tag">{spaTranslate("APP")}</span>
        </div>
        <div className="embed-card" style={{ borderLeftColor: "#ff6b4a" }}>
          <div className="embed-main">
            <p className="embed-title">{draft.title.trim() || "Untitled panel"}</p>
            <div className="embed-description">
              {draft.description.trim() && <span>{draft.description.trim()}</span>}
              {rows.length === 0 && <span className="muted">{spaTranslate("No roles yet.")}</span>}
              {rows.map((r) => (
                <span key={r.key} className="preview-line">
                  {r.emoji && (
                    <span className="preview-emoji">
                      <Emoji text={r.emoji} />
                    </span>
                  )}
                  <strong>{r.label || "Label"}</strong>·
                  <span className="preview-mention" style={{ color: color(r.role_id) }}>
                    {name(r.role_id)}
                  </span>
                  {r.description && draft.style !== "select" && (
                    <span className="muted">— {r.description}</span>
                  )}
                  {r.temp_secs ? <span className="muted">· ⏳ {spanText(r.temp_secs)}</span> : null}
                </span>
              ))}
              {draft.required_role && (
                <span className="muted">
                  {" "}
                  {spaTranslate("Needs")} <span className="preview-mention">{name(draft.required_role)}</span>
                </span>
              )}
            </div>
            <p className="embed-footer muted">{footerOf(draft)}</p>
          </div>
        </div>
        {draft.style === "buttons" && rows.length > 0 && (
          <div className="preview-buttons" role="group" aria-label={spaTranslate("Buttons")}>
            {rows.map((r) => (
              <span key={r.key} className="preview-button">
                {r.emoji && <Emoji text={r.emoji} />}
                {r.label || "Label"}
              </span>
            ))}
          </div>
        )}
        {draft.style === "select" && rows.length > 0 && (
          <>
            <div className="preview-select" role="group" aria-label={spaTranslate("Menu")}>
              <span>{spaTranslate("Pick your roles")}</span>
              <span aria-hidden="true">▾</span>
            </div>
            <div className="preview-select-menu">
              {rows.slice(0, 4).map((r) => (
                <div key={r.key}>
                  <span>
                    {r.emoji && (
                      <>
                        <Emoji text={r.emoji} />{" "}
                      </>
                    )}
                    {r.label || "Label"}
                  </span>
                  {r.description && <small>{r.description}</small>}
                </div>
              ))}
              {rows.length > 4 && (
                <small>
                  {spaTranslate("and")} {rows.length - 4} {spaTranslate("more…")}
                </small>
              )}
            </div>
          </>
        )}
        {draft.style === "reactions" && rows.length > 0 && (
          <div className="preview-reactions" role="group" aria-label={spaTranslate("Reactions")}>
            {rows.map((r) => (
              <span key={r.key} className="preview-reaction">
                {r.emoji ? <Emoji text={r.emoji} /> : "?"} 1
              </span>
            ))}
          </div>
        )}
      </div>
      <p className="preview-note muted">
        {" "}
        {spaTranslate(
          "Drawn here as Discord would show it. Nothing is posted until you press Post panel.",
        )}{" "}
      </p>
    </div>
  );
}
