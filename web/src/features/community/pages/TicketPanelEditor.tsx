import { t as deferredText } from "../../../i18n/dashboard";
import { type KeyboardEvent, useId, useRef, useState } from "react";
import { PixelButton } from "../../../components/PixelButton";
import { translate as spaTranslate } from "../../../i18n";
import { ApiError, type FieldError, type GuildInfo } from "../../../lib/api";
import {
  type Limits,
  type Panel as TicketPanel,
  type PanelDraft,
  type Question,
  type Topic,
  checkPanel,
  useSavePanel,
} from "../api/tickets";
import { PanelPreview } from "./TicketPanelFields";
import { BehaviourTab } from "./TicketBehaviourTab";
import { ButtonsTab } from "./TicketButtonsTab";
import { FormTab } from "./TicketFormTab";
import { LogTab } from "./TicketLogTab";
import type { TabProps } from "./TicketPanelTabProps";

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

export function PanelEditor({
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
            onDone(
              deferredText("Saved “{arg1}”.{arg2}", {
                arg1: saved.name,
                arg2: saved.message_id ? deferredText(" Update the post to show it.") : "",
              }),
              true,
            );
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
  const tabProps: TabProps = { draft, set, limits, info, errors, base, most, updateTopic, updateQuestion };

  return (
    <section
      className="tk-editor"
      aria-label={panel ? deferredText("Edit panel {arg1}", { arg1: panel.name }) : spaTranslate("New panel")}
    >
      <div>
        <div className="tk-tabs" role="tablist" aria-label={deferredText("Panel settings")}>
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
                    <span className="sr-only"> {deferredText("(needs a fix)")}</span>
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
          {tab === "buttons" && <ButtonsTab {...tabProps} />}
          {tab === "behaviour" && <BehaviourTab {...tabProps} />}
          {tab === "form" && <FormTab {...tabProps} />}
          {tab === "log" && <LogTab {...tabProps} />}
        </div>
      </div>
      <div className="tk-editor-side">
        <div className="tk-editor-preview settings-preview">
          <span className="field-label" id={`${base}-preview`}>
            {spaTranslate("Preview")}
          </span>
          <PanelPreview draft={draft} labelledBy={`${base}-preview`} />
          <p className="muted fine">
            {" "}
            {deferredText(
              "This is how the panel looks in Discord. Nothing is posted until you post it.",
            )}{" "}
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
              ? spaTranslate("Fix the marked field first")
              : deferredText("Fix the {arg1} marked fields first", { arg1: local.length })}
          </span>
        )}
        <PixelButton variant="ghost" onClick={onCancel}>
          {dirty ? spaTranslate("Discard") : spaTranslate("Close")}
        </PixelButton>
        <PixelButton
          variant="secondary"
          onClick={() => submit(false)}
          disabled={save.isPending || local.length > 0 || (!dirty && !!panel)}
        >
          {save.isPending ? spaTranslate("Saving…") : spaTranslate("Save panel")}
        </PixelButton>
        <PixelButton onClick={() => submit(true)} disabled={save.isPending || local.length > 0}>
          {panel?.message_id ? deferredText("Save and update post") : deferredText("Save and post")}
        </PixelButton>
      </div>
    </section>
  );
}
