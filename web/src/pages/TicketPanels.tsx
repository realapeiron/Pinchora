import { type KeyboardEvent, useId, useRef, useState } from "react";

import { Choice } from "../components/Choice";
import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { ChannelPicker, ChannelTag, RolePicker, RoleTag } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { RichText } from "../components/RichText";
import { ApiError, type FieldError, type GuildInfo } from "../lib/api";
import {
  type Limits,
  type Panel as TicketPanel,
  type PanelDraft,
  type Question,
  type Tone,
  type Topic,
  checkPanel,
  draftOf,
  newPanel,
  useDeletePanel,
  usePostPanel,
  useSavePanel,
} from "../lib/tickets";

const TONES: { value: Tone; label: string }[] = [
  { value: "primary", label: "Blurple" },
  { value: "secondary", label: "Grey" },
  { value: "success", label: "Green" },
  { value: "danger", label: "Red" },
];

const WELCOME_PLACEHOLDERS = ["{user}", "{server}", "{number}", "{topic}"];

/** A `<select>` of the server's categories. */
export function CategoryPicker({
  id,
  info,
  value,
  onChange,
  none,
  invalid,
}: {
  id: string;
  info: GuildInfo | undefined;
  value: string | null;
  onChange: (id: string | null) => void;
  none: string;
  invalid?: boolean;
}) {
  const categories = (info?.channels ?? []).filter((c) => c.type === 4);
  const known = !value || categories.some((c) => c.id === value);
  return (
    <select
      id={id}
      className="pixel-input pixel-select"
      value={value ?? ""}
      onChange={(event) => onChange(event.target.value || null)}
      disabled={!info}
      aria-invalid={invalid || undefined}
    >
      <option value="">{none}</option>
      {!known && <option value={value!}>Deleted category ({value})</option>}
      {categories.map((c) => (
        <option key={c.id} value={c.id}>
          {`▸ ${c.name}`}
        </option>
      ))}
    </select>
  );
}

/** The server's panels: the list, and the editor of the one being changed or made. */
export function TicketPanels({
  guild,
  panels,
  limits,
  info,
  onToast,
}: {
  guild: string;
  panels: TicketPanel[];
  limits: Limits;
  info: GuildInfo | undefined;
  onToast: (message: string) => void;
}) {
  const [editing, setEditing] = useState<string | "new" | null>(null);
  const [asking, setAsking] = useState<TicketPanel | null>(null);
  const post = usePostPanel(guild);
  const remove = useDeletePanel(guild);
  const full = panels.length >= limits.panels;
  const current = editing && editing !== "new" ? panels.find((p) => p.id === editing) : undefined;
  const firstChannel = info?.channels.find((c) => c.type === 0)?.id ?? null;

  const publish = (panel: TicketPanel) =>
    post.mutate(panel.id, {
      onSuccess: (done) =>
        onToast(done.updated ? `“${panel.name}” was updated where it is.` : `“${panel.name}” is posted.`),
    });

  return (
    <Panel
      title="PANELS"
      aside={
        <span className="muted counter">
          {panels.length}/{limits.panels}
        </span>
      }
    >
      <p className="panel-lead muted">
        A panel is the message members open tickets from: up to {limits.buttons} topic buttons, or a menu of
        up to {limits.options}. Save it, then post it; posting again updates the message in place.
      </p>
      {post.isError && (
        <p className="field-error" role="alert">
          <RichText text={post.error.message} />
        </p>
      )}
      {panels.length === 0 ? (
        <p className="empty-row muted">No panels yet. Make one and post it where members ask for help.</p>
      ) : (
        <ul className="tk-panel-list">
          {panels.map((panel) => (
            <li
              key={panel.id}
              className={`tk-panel-row ${editing === panel.id ? "is-editing" : ""}`}
              data-panel={panel.name}
            >
              <div className="tk-panel-main">
                <span className="tk-panel-name">
                  {panel.name}
                  {panel.message_id ? (
                    <span className="badge badge-mint">POSTED</span>
                  ) : (
                    <span className="badge tk-badge-quiet">NOT POSTED</span>
                  )}
                  {!panel.enabled && <span className="badge badge-gold">OFF</span>}
                </span>
                <p className="tk-panel-facts">
                  <ChannelTag info={info} id={panel.channel_id} /> ·{" "}
                  {panel.style === "buttons" ? "Buttons" : "Menu"} · {panel.topics.length} topic
                  {panel.topics.length === 1 ? "" : "s"}
                  {panel.questions.length > 0 &&
                    ` · form of ${panel.questions.length} question${panel.questions.length === 1 ? "" : "s"}`}
                  {panel.support_roles.length > 0 &&
                    ` · ${panel.support_roles.length} support role${panel.support_roles.length === 1 ? "" : "s"}`}
                </p>
              </div>
              <div className="tk-panel-actions">
                <PixelButton
                  variant="secondary"
                  className="button-small"
                  onClick={() => setEditing(editing === panel.id ? null : panel.id)}
                  aria-expanded={editing === panel.id}
                  aria-label={`Edit ${panel.name}`}
                >
                  {editing === panel.id ? "Close" : "Edit"}
                </PixelButton>
                <PixelButton
                  className="button-small"
                  onClick={() => publish(panel)}
                  disabled={post.isPending}
                  aria-label={`${panel.message_id ? "Update the post of" : "Post"} ${panel.name}`}
                >
                  {panel.message_id ? "Update post" : "Post"}
                </PixelButton>
                <PixelButton
                  variant="ghost"
                  className="button-small"
                  onClick={() => setAsking(panel)}
                  aria-label={`Delete ${panel.name}`}
                >
                  Delete
                </PixelButton>
              </div>
            </li>
          ))}
        </ul>
      )}
      <div className="panel-foot">
        {full && <span className="muted fine">A server has at most {limits.panels} panels.</span>}
        <PixelButton onClick={() => setEditing("new")} disabled={full || editing === "new"}>
          New panel
        </PixelButton>
      </div>
      {editing && (
        <PanelEditor
          key={editing}
          guild={guild}
          panel={current}
          initial={current ? draftOf(current) : newPanel(firstChannel)}
          limits={limits}
          info={info}
          onDone={(message, keepOpen) => {
            onToast(message);
            if (!keepOpen) setEditing(null);
          }}
          onSaved={(saved) => setEditing(saved.id)}
          onCancel={() => setEditing(null)}
          onPost={publish}
        />
      )}
      {asking && (
        <div
          className="dialog-backdrop"
          role="presentation"
          onKeyDown={(event) => {
            if (event.key === "Escape") setAsking(null);
          }}
        >
          <div
            className="panel panel-gold dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="panel-delete-title"
          >
            <div className="panel-body">
              <h2 id="panel-delete-title" className="pixel-title">
                Delete “{asking.name}”?
              </h2>
              <p>The panel and its posted message go. Its tickets and their transcripts stay.</p>
              {remove.isError && (
                <p className="field-error" role="alert">
                  {remove.error.message}
                </p>
              )}
              <div className="dialog-actions">
                <PixelButton
                  variant="secondary"
                  autoFocus
                  onClick={() => {
                    setAsking(null);
                    remove.reset();
                  }}
                >
                  Keep it
                </PixelButton>
                <PixelButton
                  disabled={remove.isPending}
                  onClick={() =>
                    remove.mutate(asking.id, {
                      onSuccess: () => {
                        onToast(`Deleted “${asking.name}”.`);
                        if (editing === asking.id) setEditing(null);
                        setAsking(null);
                      },
                    })
                  }
                >
                  {remove.isPending ? "Deleting…" : "Delete it"}
                </PixelButton>
              </div>
            </div>
          </div>
        </div>
      )}
    </Panel>
  );
}

type TabKey = "buttons" | "behaviour" | "form" | "log";

const TABS: { key: TabKey; label: string; fields: string[] }[] = [
  {
    key: "buttons",
    label: "Buttons",
    fields: ["name", "channel_id", "title", "description", "color", "buttons", "style"],
  },
  { key: "behaviour", label: "Behaviour", fields: ["support_roles", "welcome_text", "open_category_id"] },
  { key: "form", label: "Form", fields: ["questions"] },
  { key: "log", label: "Log", fields: ["log_channel_id"] },
];

/** Equal drafts, field by field. */
function same(a: PanelDraft, b: PanelDraft): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function PanelEditor({
  guild,
  panel,
  initial,
  limits,
  info,
  onDone,
  onSaved,
  onCancel,
  onPost,
}: {
  guild: string;
  panel: TicketPanel | undefined;
  initial: PanelDraft;
  limits: Limits;
  info: GuildInfo | undefined;
  onDone: (message: string, keepOpen?: boolean) => void;
  onSaved: (panel: TicketPanel) => void;
  onCancel: () => void;
  onPost: (panel: TicketPanel) => void;
}) {
  const [draft, setDraft] = useState<PanelDraft>(initial);
  const [tab, setTab] = useState<TabKey>("buttons");
  const [serverErrors, setServerErrors] = useState<FieldError[]>([]);
  const save = useSavePanel(guild);
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);
  const base = useId();
  const local = checkPanel(draft, limits);
  const errors: Record<string, string> = {};
  for (const e of [...serverErrors, ...local]) errors[e.field] ??= e.message;
  const dirty = !same(draft, initial);
  const set = <K extends keyof PanelDraft>(field: K, value: PanelDraft[K]) => {
    setDraft((d) => ({ ...d, [field]: value }));
    setServerErrors((list) =>
      list.filter((e) => e.field !== field && !(field === "topics" && e.field === "buttons")),
    );
  };
  const most = draft.style === "buttons" ? limits.buttons : limits.options;

  const submit = (andPost: boolean) =>
    save.mutate(
      { id: panel?.id ?? null, draft },
      {
        onSuccess: (saved) => {
          setServerErrors([]);
          onSaved(saved);
          if (andPost) onPost(saved);
          else
            onDone(`Saved “${saved.name}”.${saved.message_id ? " Update the post to show it." : ""}`, true);
        },
        onError: (error) => {
          const list = error instanceof ApiError ? error.errors : [];
          setServerErrors(list);
          const first = TABS.find((t) => list.some((e) => t.fields.includes(e.field)));
          if (first) setTab(first.key);
        },
      },
    );

  const onTabKey = (event: KeyboardEvent<HTMLButtonElement>, at: number) => {
    const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (!step && event.key !== "Home" && event.key !== "End") return;
    event.preventDefault();
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? TABS.length - 1
          : (at + step + TABS.length) % TABS.length;
    setTab(TABS[next]!.key);
    tabs.current[next]?.focus();
  };

  const updateTopic = (n: number, patch: Partial<Topic>) =>
    set(
      "topics",
      draft.topics.map((t, i) => (i === n ? { ...t, ...patch } : t)),
    );
  const updateQuestion = (n: number, patch: Partial<Question>) =>
    set(
      "questions",
      draft.questions.map((q, i) => (i === n ? { ...q, ...patch } : q)),
    );

  return (
    <section className="tk-editor" aria-label={panel ? `Edit panel ${panel.name}` : "New panel"}>
      <div>
        <div className="tk-tabs" role="tablist" aria-label="Panel settings">
          {TABS.map((t, n) => {
            const broken = Object.keys(errors).some((field) => t.fields.includes(field));
            return (
              <button
                key={t.key}
                ref={(el) => {
                  tabs.current[n] = el;
                }}
                id={`${base}-tab-${t.key}`}
                type="button"
                role="tab"
                className="tk-tab"
                aria-selected={tab === t.key}
                aria-controls={`${base}-panel-${t.key}`}
                tabIndex={tab === t.key ? 0 : -1}
                onClick={() => setTab(t.key)}
                onKeyDown={(event) => onTabKey(event, n)}
              >
                {t.label}
                {broken && (
                  <>
                    <span className="tk-tab-dot" aria-hidden="true" />
                    <span className="sr-only"> (needs a fix)</span>
                  </>
                )}
              </button>
            );
          })}
        </div>
        <div
          className="tk-tabpanel"
          role="tabpanel"
          id={`${base}-panel-${tab}`}
          aria-labelledby={`${base}-tab-${tab}`}
        >
          {tab === "buttons" && (
            <>
              <div className="form-grid form-grid-2">
                <Field
                  label="Name"
                  htmlFor={`${base}-name`}
                  error={errors.name}
                  hint="How /setup tickets and this page find it. Members never see it."
                >
                  <input
                    id={`${base}-name`}
                    className="pixel-input"
                    value={draft.name}
                    maxLength={limits.name}
                    onChange={(e) => set("name", e.target.value)}
                    aria-invalid={errors.name ? true : undefined}
                  />
                </Field>
                <Field
                  label="Posted in"
                  htmlFor={`${base}-channel`}
                  error={errors.channel_id}
                  hint="Private threads open off this channel in thread mode."
                >
                  <ChannelPicker
                    id={`${base}-channel`}
                    info={info}
                    value={draft.channel_id}
                    onChange={(id) => set("channel_id", id)}
                    invalid={!!errors.channel_id}
                  />
                </Field>
              </div>
              <Field
                label="Title"
                htmlFor={`${base}-title`}
                error={errors.title}
                aside={
                  <span className="counter">
                    {[...draft.title].length}/{limits.title}
                  </span>
                }
              >
                <input
                  id={`${base}-title`}
                  className="pixel-input"
                  value={draft.title}
                  onChange={(e) => set("title", e.target.value)}
                  aria-invalid={errors.title ? true : undefined}
                />
              </Field>
              <Field
                label="Text"
                htmlFor={`${base}-description`}
                error={errors.description}
                hint="Markdown works: **bold**, *italic*. Mentions are dropped."
                aside={
                  <span className="counter">
                    {[...draft.description].length}/{limits.description}
                  </span>
                }
              >
                <textarea
                  id={`${base}-description`}
                  className="pixel-input pixel-textarea"
                  value={draft.description}
                  onChange={(e) => set("description", e.target.value)}
                  aria-invalid={errors.description ? true : undefined}
                />
              </Field>
              <Field
                label="Colour"
                htmlFor={`${base}-color`}
                error={errors.color}
                hint="The stripe down the side."
              >
                <div className="tk-color">
                  <input
                    type="color"
                    aria-label="Pick the colour"
                    value={/^#[0-9a-fA-F]{6}$/.test(draft.color) ? draft.color : "#ff6b4a"}
                    onChange={(e) => set("color", e.target.value)}
                  />
                  <input
                    id={`${base}-color`}
                    className="pixel-input input-short"
                    value={draft.color}
                    maxLength={7}
                    spellCheck={false}
                    onChange={(e) => set("color", e.target.value)}
                    aria-invalid={errors.color ? true : undefined}
                  />
                </div>
              </Field>
              <Field label="Style" htmlFor={`${base}-style-buttons`}>
                <Choice
                  name="Panel style"
                  value={draft.style}
                  onChange={(style) => set("style", style)}
                  options={[
                    {
                      value: "buttons",
                      title: "Buttons",
                      detail: `One button per topic, up to ${limits.buttons}.`,
                    },
                    {
                      value: "select",
                      title: "Menu",
                      detail: `A drop-down of up to ${limits.options} topics.`,
                    },
                  ]}
                />
              </Field>
              <Field
                label="Topics"
                error={errors.buttons}
                aside={
                  <span className={`counter ${draft.topics.length > most ? "is-over" : ""}`}>
                    {draft.topics.length}/{most}
                  </span>
                }
              >
                <ul className="tk-topics">
                  {draft.topics.map((topic, n) => (
                    <li key={n} className={`tk-topic ${draft.style === "select" ? "is-menu" : ""}`}>
                      <input
                        className="pixel-input tk-emoji"
                        aria-label={`Topic ${n + 1} emoji`}
                        placeholder="🎫"
                        value={topic.emoji ?? ""}
                        maxLength={8}
                        onChange={(e) => updateTopic(n, { emoji: e.target.value })}
                      />
                      <input
                        className="pixel-input"
                        aria-label={`Topic ${n + 1} label`}
                        value={topic.label}
                        maxLength={limits.label}
                        onChange={(e) => updateTopic(n, { label: e.target.value })}
                      />
                      {draft.style === "buttons" ? (
                        <select
                          className="pixel-input pixel-select"
                          aria-label={`Topic ${n + 1} colour`}
                          value={topic.tone}
                          onChange={(e) => updateTopic(n, { tone: e.target.value as Tone })}
                        >
                          {TONES.map((t) => (
                            <option key={t.value} value={t.value}>
                              {t.label}
                            </option>
                          ))}
                        </select>
                      ) : (
                        <input
                          className="pixel-input"
                          aria-label={`Topic ${n + 1} description`}
                          placeholder="A line under it (optional)"
                          value={topic.description ?? ""}
                          maxLength={limits.option_description}
                          onChange={(e) => updateTopic(n, { description: e.target.value })}
                        />
                      )}
                      <button
                        type="button"
                        className="tk-icon-button"
                        aria-label={`Remove topic ${n + 1}`}
                        onClick={() =>
                          set(
                            "topics",
                            draft.topics.filter((_, i) => i !== n),
                          )
                        }
                      >
                        ×
                      </button>
                    </li>
                  ))}
                </ul>
                <PixelButton
                  variant="secondary"
                  className="button-small tk-add"
                  disabled={draft.topics.length >= most}
                  onClick={() =>
                    set("topics", [
                      ...draft.topics,
                      { label: "", emoji: null, description: null, tone: "primary" },
                    ])
                  }
                >
                  Add topic
                </PixelButton>
              </Field>
            </>
          )}
          {tab === "behaviour" && (
            <>
              <Field
                label="Support roles"
                htmlFor={`${base}-roles`}
                error={errors.support_roles}
                hint="They see these tickets, get pinged in new ones and may claim, lock and close them. Manage Server and Manage Channels always may."
                aside={
                  <span className="counter">
                    {draft.support_roles.length}/{limits.support_roles}
                  </span>
                }
              >
                <SupportRoles
                  id={`${base}-roles`}
                  info={info}
                  ids={draft.support_roles}
                  max={limits.support_roles}
                  onChange={(ids) => set("support_roles", ids)}
                />
              </Field>
              <Field
                label="Welcome text"
                htmlFor={`${base}-welcome`}
                error={errors.welcome_text}
                hint="The ticket's first message. Empty uses a friendly default."
                aside={
                  <span className="counter">
                    {[...draft.welcome_text].length}/{limits.welcome}
                  </span>
                }
              >
                <textarea
                  id={`${base}-welcome`}
                  className="pixel-input pixel-textarea"
                  placeholder="Hi {user}, thanks for reaching out. Tell us what's going on and a staff member will be with you soon."
                  value={draft.welcome_text}
                  onChange={(e) => set("welcome_text", e.target.value)}
                  aria-invalid={errors.welcome_text ? true : undefined}
                />
                <div className="chips" role="group" aria-label="Insert a placeholder">
                  {WELCOME_PLACEHOLDERS.map((token) => (
                    <button
                      key={token}
                      type="button"
                      className="chip"
                      onClick={() => set("welcome_text", `${draft.welcome_text}${token}`)}
                    >
                      {token}
                    </button>
                  ))}
                </div>
              </Field>
              <Field
                label="Category"
                htmlFor={`${base}-category`}
                error={errors.open_category_id}
                hint="Where this panel's ticket channels go, if not the server's ticket category."
              >
                <CategoryPicker
                  id={`${base}-category`}
                  info={info}
                  value={draft.open_category_id}
                  onChange={(id) => set("open_category_id", id)}
                  none="The server's ticket category"
                  invalid={!!errors.open_category_id}
                />
              </Field>
              <SwitchRow
                title="Takes tickets"
                control={
                  <PixelToggle
                    label="Panel takes tickets"
                    checked={draft.enabled}
                    onChange={(on) => set("enabled", on)}
                  />
                }
              >
                Off, its buttons are greyed out once you update the post.
              </SwitchRow>
            </>
          )}
          {tab === "form" && (
            <Field
              label="Questions"
              error={errors.questions}
              hint="Asked in a form before the ticket opens; the answers are the ticket's first message. No questions, no form."
              aside={
                <span className="counter">
                  {draft.questions.length}/{limits.questions}
                </span>
              }
            >
              {draft.questions.length === 0 ? (
                <p className="empty-row muted">No form: the ticket opens straight away.</p>
              ) : (
                <ol className="tk-questions">
                  {draft.questions.map((q, n) => (
                    <li key={n} className="tk-question">
                      <input
                        className="pixel-input"
                        aria-label={`Question ${n + 1}`}
                        placeholder="What is your order number?"
                        value={q.label}
                        maxLength={limits.question}
                        onChange={(e) => updateQuestion(n, { label: e.target.value })}
                      />
                      <input
                        className="pixel-input"
                        aria-label={`Question ${n + 1} example`}
                        placeholder="An example answer (optional)"
                        value={q.placeholder}
                        maxLength={limits.placeholder}
                        onChange={(e) => updateQuestion(n, { placeholder: e.target.value })}
                      />
                      <button
                        type="button"
                        className="tk-icon-button"
                        aria-label={`Remove question ${n + 1}`}
                        onClick={() =>
                          set(
                            "questions",
                            draft.questions.filter((_, i) => i !== n),
                          )
                        }
                      >
                        ×
                      </button>
                      <div className="tk-question-switches">
                        <label className="tk-check">
                          <input
                            type="checkbox"
                            checked={q.long}
                            onChange={(e) => updateQuestion(n, { long: e.target.checked })}
                          />
                          Long answer
                        </label>
                        <label className="tk-check">
                          <input
                            type="checkbox"
                            checked={q.required}
                            onChange={(e) => updateQuestion(n, { required: e.target.checked })}
                          />
                          Required
                        </label>
                      </div>
                    </li>
                  ))}
                </ol>
              )}
              <PixelButton
                variant="secondary"
                className="button-small tk-add"
                disabled={draft.questions.length >= limits.questions}
                onClick={() =>
                  set("questions", [
                    ...draft.questions,
                    { label: "", placeholder: "", long: false, required: true },
                  ])
                }
              >
                Add question
              </PixelButton>
            </Field>
          )}
          {tab === "log" && (
            <>
              <Field
                label="Log channel"
                htmlFor={`${base}-log`}
                error={errors.log_channel_id}
                hint="Opened, claimed, reopened and closed tickets of this panel, with the transcript, if not the server's log channel."
              >
                <ChannelPicker
                  id={`${base}-log`}
                  info={info}
                  value={draft.log_channel_id}
                  onChange={(id) => set("log_channel_id", id)}
                  none="The server's log channel"
                  invalid={!!errors.log_channel_id}
                />
              </Field>
              <p className="notice" role="note">
                Transcripts by direct message and ratings are server settings, above in{" "}
                <strong>Settings</strong>.
              </p>
            </>
          )}
        </div>
      </div>
      <div className="tk-editor-side">
        <div className="tk-editor-preview">
          <span className="field-label" id={`${base}-preview`}>
            Preview
          </span>
          <PanelPreview draft={draft} labelledBy={`${base}-preview`} />
          <p className="muted fine">
            This is how the panel looks in Discord. Nothing is posted until you post it.
          </p>
        </div>
      </div>
      <div className="panel-foot tk-editor-foot">
        {save.isError && !(save.error instanceof ApiError && save.error.errors.length > 0) && (
          <span className="field-error" role="alert">
            {save.error.message}
          </span>
        )}
        {local.length > 0 && dirty && (
          <span className="muted fine" role="status">
            {local.length === 1
              ? "Fix the marked field first"
              : `Fix the ${local.length} marked fields first`}
          </span>
        )}
        <PixelButton variant="ghost" onClick={onCancel}>
          {dirty ? "Discard" : "Close"}
        </PixelButton>
        <PixelButton
          variant="secondary"
          onClick={() => submit(false)}
          disabled={save.isPending || local.length > 0 || (!dirty && !!panel)}
        >
          {save.isPending ? "Saving…" : "Save panel"}
        </PixelButton>
        <PixelButton onClick={() => submit(true)} disabled={save.isPending || local.length > 0}>
          {panel?.message_id ? "Save and update post" : "Save and post"}
        </PixelButton>
      </div>
    </section>
  );
}

/** The panel's support roles as chips, with a labelled picker to add one. */
function SupportRoles({
  id,
  info,
  ids,
  max,
  onChange,
}: {
  id: string;
  info: GuildInfo | undefined;
  ids: string[];
  max: number;
  onChange: (next: string[]) => void;
}) {
  const [pick, setPick] = useState<string | null>(null);
  const full = ids.length >= max;
  const nameOf = (role: string) => info?.roles.find((r) => r.id === role)?.name ?? role;
  return (
    <div className="id-list">
      {ids.length === 0 ? (
        <p className="muted id-list-empty">No support role: only managers see the tickets.</p>
      ) : (
        <ul className="channel-chips" aria-label="Support roles">
          {ids.map((role) => (
            <li key={role} className="channel-chip">
              <RoleTag info={info} id={role} />
              <button
                type="button"
                className="chip-remove"
                aria-label={`Remove ${nameOf(role)}`}
                onClick={() => onChange(ids.filter((x) => x !== role))}
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="module-add">
        <RolePicker
          id={id}
          info={info}
          value={pick}
          onChange={setPick}
          none={full ? `Up to ${max}` : "Pick a role"}
          disabled={full}
        />
        <PixelButton
          variant="secondary"
          className="button-small"
          disabled={!pick || full || ids.includes(pick)}
          onClick={() => {
            if (pick) onChange([...ids, pick]);
            setPick(null);
          }}
        >
          Add role
        </PixelButton>
      </div>
    </div>
  );
}

/** The panel as Discord shows it: the bot, the embed, the buttons or the menu. */
export function PanelPreview({ draft, labelledBy }: { draft: PanelDraft; labelledBy?: string }) {
  const color = /^#?[0-9a-fA-F]{6}$/.test(draft.color.trim())
    ? `#${draft.color.trim().replace(/^#/, "")}`
    : "#ff6b4a";
  const topics = draft.topics.filter((t) => t.label.trim());
  return (
    <div className={`discord ${draft.enabled ? "" : "dc-off"}`} role="region" aria-labelledby={labelledBy}>
      <div className="discord-head">
        <span className="discord-avatar" aria-hidden="true" />
        <strong>Pinchora</strong>
        <span className="discord-tag">APP</span>
      </div>
      <div className="embed-card" style={{ borderLeftColor: color }}>
        <div className="embed-main">
          <p className="embed-title">
            {draft.title.trim() ? <RichText text={draft.title} /> : "Untitled panel"}
          </p>
          {draft.description.trim() && (
            <p className="embed-description">
              <RichText text={draft.description} />
            </p>
          )}
          <p className="embed-footer muted">Pick a topic to open a private ticket.</p>
        </div>
      </div>
      {draft.style === "buttons" ? (
        <div className="dc-buttons" aria-label="Buttons">
          {topics.slice(0, 5).map((t, n) => (
            <span key={n} className={`dc-button is-${t.tone}`}>
              {t.emoji && <span aria-hidden="true">{t.emoji}</span>}
              {t.label}
            </span>
          ))}
        </div>
      ) : (
        <>
          <div className="dc-select" aria-hidden="true">
            <span>What do you need help with?</span>
            <span>▾</span>
          </div>
          <ul className="dc-options" aria-label="Menu options">
            {topics.slice(0, 25).map((t, n) => (
              <li key={n}>
                <span>
                  {t.emoji && <span aria-hidden="true">{t.emoji} </span>}
                  {t.label}
                </span>
                {t.description && <small>{t.description}</small>}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
