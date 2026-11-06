import { useMemo } from "react";
import { useNavigate, useOutletContext, useParams } from "react-router";

import { ActionsEditor } from "../components/ActionsEditor";
import { Choice } from "../components/Choice";
import { EmbedEditor } from "../components/EmbedEditor";
import { Field, SwitchRow } from "../components/Field";
import { IdList } from "../components/IdList";
import { Panel } from "../components/Panel";
import { ChannelPicker, RolePicker } from "../components/Pickers";
import { PixelLink } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { PreviewPanel } from "../components/PreviewPanel";
import { ResponsesEditor } from "../components/ResponsesEditor";
import { SaveBar } from "../components/SaveBar";
import type { Overview, ResponderDef, TriggerKind } from "../lib/api";
import { useDiscord, useMe, useSaveTag, useTag } from "../lib/queries";
import {
  LIMITS,
  MATCH_KINDS,
  TRIGGERS,
  actionsProblem,
  blankResponder,
  destinationsFor,
  embedIsBlank,
  finishedActions,
  placeholdersFor,
  responderDefOf,
  tidyEmbed,
} from "../lib/tags";
import { useTagEditor } from "../lib/useTagEditor";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

function tidyResponder(draft: ResponderDef): ResponderDef {
  const value = draft.trigger_value?.trim() ?? "";
  return {
    ...draft,
    name: draft.name.trim(),
    trigger_value: value === "" ? null : value,
    responses: draft.responses.map((r) => r.trim()),
    embed: tidyEmbed(draft.embed),
    channel_id: draft.destination === "channel" ? draft.channel_id : null,
  };
}

/** `/g/:guild/responders/new` and `/g/:guild/responders/:responderId`. */
export function ResponderEditor() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const { responderId } = useParams();
  const query = useTag(guild, "responders", responderId ?? null);
  if (responderId !== undefined && query.isPending) return <Loading label="Opening the responder" />;
  if (responderId !== undefined && query.isError) return <Problem error={query.error} />;
  return (
    <ResponderForm
      key={responderId ?? "new"}
      guild={guild}
      id={query.data?.id}
      initial={query.data ? responderDefOf(query.data) : blankResponder()}
    />
  );
}

function ResponderForm({ guild, id, initial }: { guild: string; id?: string; initial: ResponderDef }) {
  const navigate = useNavigate();
  const info = useDiscord(guild).data;
  const listening = useMe().data?.capabilities.message_content ?? false;
  const submit = useSaveTag(guild, "responders");
  const editor = useTagEditor(initial, tidyResponder, (def) => submit.mutateAsync({ id, def }));
  const { draft, set, patch } = editor;
  const kind = draft.trigger_kind;
  const keywordLocked = !listening;
  const hasEmbed = draft.embed !== null && !embedIsBlank(draft.embed);
  const ready = draft.responses.some((r) => r.trim() !== "") || hasEmbed;
  const body = useMemo(
    () => ({
      responder: {
        ...tidyResponder(draft),
        name: draft.name.trim() || "preview",
        actions: finishedActions(draft.actions),
      },
    }),
    [draft],
  );
  const unfinished = actionsProblem(draft.actions);
  const nameMissing = draft.name.trim() === "";

  /** A different trigger: what hung on the old one (its value, where it answers, its actions) is reset. */
  const pickTrigger = (next: TriggerKind) => {
    const fresh = blankResponder(next);
    patch({
      trigger_kind: next,
      trigger_value: null,
      match_kind: fresh.match_kind,
      // Where it answers follows the trigger unless the author picked something else that still works.
      destination:
        draft.destination !== blankResponder(kind).destination &&
        destinationsFor(next).some((d) => d.value === draft.destination)
          ? draft.destination
          : fresh.destination,
      actions: draft.actions.filter((a) =>
        next === "member_leave" ? false : a.kind !== "react" || next === "keyword",
      ),
      enabled: next === "keyword" && keywordLocked ? false : draft.enabled,
    });
  };

  const onSave = () => {
    if (nameMissing || unfinished) return;
    void editor.save(() =>
      navigate(`/g/${guild}/responders`, {
        state: { toast: `Saved “${draft.name.trim()}”. The bot uses it from the next event.` },
      }),
    );
  };

  const hearsChat = kind === "keyword" || kind === "level_up";
  return (
    <div className="settings-page">
      <PageHead
        title={id ? "EDIT RESPONDER" : "NEW RESPONDER"}
        lead="Sent by the bot by itself when the trigger happens."
        action={
          <PixelLink variant="secondary" href={`/g/${guild}/responders`}>
            Back to the list
          </PixelLink>
        }
      />
      <div className="editor">
        <div className="editor-form">
          <Panel title="BASICS">
            <Field
              label="Name"
              htmlFor="responder-name"
              error={editor.problem("name")}
              hint="Only for you, to tell responders apart."
            >
              <input
                id="responder-name"
                className="pixel-input"
                value={draft.name}
                maxLength={LIMITS.responder_name_max}
                autoComplete="off"
                onChange={(event) => set("name", event.target.value)}
              />
            </Field>
            <SwitchRow
              title="Switched on"
              control={
                <PixelToggle
                  label="Switched on"
                  checked={draft.enabled}
                  disabled={kind === "keyword" && keywordLocked && !draft.enabled}
                  onChange={(next) => set("enabled", next)}
                />
              }
            >
              {kind === "keyword" && keywordLocked
                ? "Keyword triggers can't be switched on while the bot can't read chat."
                : "Off, it stays written but the bot never sends it."}
            </SwitchRow>
          </Panel>

          <Panel title="WHEN IT FIRES">
            <div className="choice choice-triggers" role="radiogroup" aria-label="Trigger">
              {TRIGGERS.map((t) => {
                const locked = t.needs_message_content && keywordLocked && kind !== t.key;
                return (
                  <label
                    key={t.key}
                    className={`choice-card ${kind === t.key ? "is-checked" : ""} ${locked ? "is-locked" : ""}`}
                  >
                    <input
                      type="radio"
                      name="Trigger"
                      value={t.key}
                      checked={kind === t.key}
                      disabled={locked}
                      onChange={() => pickTrigger(t.key)}
                    />
                    <span className="choice-title">
                      {t.label}
                      {t.needs_message_content && keywordLocked && (
                        <span className="badge badge-gold choice-lock">LOCKED</span>
                      )}
                    </span>
                    {t.needs_message_content && keywordLocked && (
                      <span className="choice-detail muted" data-testid="keyword-reason">
                        Needs the Message Content intent, which this bot runs without.
                      </span>
                    )}
                  </label>
                );
              })}
            </div>
            {(kind === "role_added" || kind === "role_removed") && (
              <Field label="Which role" htmlFor="responder-role" error={editor.problem("trigger_value")}>
                <RolePicker
                  id="responder-role"
                  info={info}
                  value={draft.trigger_value}
                  onChange={(next) => set("trigger_value", next)}
                  none="Pick a role"
                />
              </Field>
            )}
            {kind === "level_up" && (
              <Field
                label="From level"
                htmlFor="responder-level"
                error={editor.problem("trigger_value")}
                hint="Fires for every level-up from this level on. Empty: every level-up."
              >
                <input
                  id="responder-level"
                  type="number"
                  className="pixel-input input-short"
                  min={1}
                  max={1000}
                  value={draft.trigger_value ?? ""}
                  onChange={(event) =>
                    set("trigger_value", event.target.value === "" ? null : event.target.value)
                  }
                />
              </Field>
            )}
            {kind === "keyword" && (
              <>
                <div className="form-grid form-grid-2">
                  <Field
                    label="Word or phrase"
                    htmlFor="responder-keyword"
                    error={editor.problem("trigger_value")}
                    hint="Capital letters don't matter."
                  >
                    <input
                      id="responder-keyword"
                      className="pixel-input"
                      value={draft.trigger_value ?? ""}
                      maxLength={LIMITS.keyword_max}
                      onChange={(event) => set("trigger_value", event.target.value)}
                    />
                  </Field>
                  <Field
                    label="How it matches"
                    htmlFor="responder-match"
                    error={editor.problem("match_kind")}
                  >
                    <select
                      id="responder-match"
                      className="pixel-input pixel-select"
                      value={draft.match_kind ?? "contains"}
                      onChange={(event) =>
                        set("match_kind", event.target.value as ResponderDef["match_kind"])
                      }
                    >
                      {MATCH_KINDS.map((m) => (
                        <option key={m.value} value={m.value}>
                          {m.title}: {m.detail}
                        </option>
                      ))}
                    </select>
                  </Field>
                </div>
                <p className="field-hint muted">
                  One answer per message at most: if several responders match, the first that is ready
                  answers.
                </p>
              </>
            )}
          </Panel>

          <Panel title="REPLY">
            <ResponsesEditor
              responses={draft.responses}
              onChange={(next) => set("responses", next)}
              placeholders={placeholdersFor(kind)}
              error={editor.problem("responses")}
              optional={hasEmbed}
            />
            <EmbedEditor
              embed={draft.embed}
              onChange={(next) => set("embed", next)}
              error={editor.problem("embed")}
            />
            <SwitchRow
              title="Allow mentions"
              control={
                <PixelToggle
                  label="Allow mentions"
                  checked={draft.allow_mentions}
                  onChange={(next) => set("allow_mentions", next)}
                />
              }
            >
              Off, people and roles you write in the text show as plain words and only the member it is about
              is pinged. @everyone and @here never are.
            </SwitchRow>
          </Panel>

          <Panel title="WHERE IT ANSWERS">
            <Choice
              name="Where it answers"
              value={draft.destination}
              options={destinationsFor(kind)}
              onChange={(next) => set("destination", next)}
            />
            {draft.destination === "channel" && (
              <Field label="Channel" htmlFor="responder-channel" error={editor.problem("channel_id")}>
                <ChannelPicker
                  id="responder-channel"
                  info={info}
                  value={draft.channel_id}
                  onChange={(next) => set("channel_id", next)}
                />
              </Field>
            )}
            {editor.problem("destination") && (
              <p className="field-error" role="alert">
                {editor.problem("destination")}
              </p>
            )}
          </Panel>

          <Panel title="ACTIONS">
            <ActionsEditor
              actions={draft.actions}
              onChange={(next) => set("actions", next)}
              info={info}
              trigger={kind}
              error={editor.problem("actions") ?? unfinished}
            />
          </Panel>

          <Panel title="HOW OFTEN">
            <div className="form-grid form-grid-2">
              <Field
                label="Chance"
                htmlFor="responder-chance"
                error={editor.problem("chance")}
                hint="Percent of the times it fires that it answers."
              >
                <input
                  id="responder-chance"
                  type="number"
                  className="pixel-input input-short"
                  min={1}
                  max={100}
                  value={draft.chance}
                  onChange={(event) => set("chance", Number(event.target.value))}
                />
              </Field>
              <Field
                label="Cooldown"
                htmlFor="responder-cooldown"
                error={editor.problem("cooldown_secs")}
                hint={`Seconds before it can answer the same member again, up to ${LIMITS.responder_cooldown_max}.`}
              >
                <input
                  id="responder-cooldown"
                  type="number"
                  className="pixel-input input-short"
                  min={0}
                  max={LIMITS.responder_cooldown_max}
                  value={draft.cooldown_secs}
                  onChange={(event) => set("cooldown_secs", Number(event.target.value))}
                />
              </Field>
            </div>
            {hearsChat && (
              <div className="form-grid form-grid-2">
                <Field
                  label="Only in these channels"
                  error={editor.problem("allow_channels")}
                  hint="Where the message was sent, or the member levelled up. A category covers the channels under it."
                >
                  <IdList
                    kind="channel"
                    info={info}
                    ids={draft.allow_channels}
                    onChange={(next) => set("allow_channels", next)}
                    max={LIMITS.max_list}
                    name="Allowed channels"
                    none="Every channel."
                  />
                </Field>
                <Field label="Never in these channels" error={editor.problem("deny_channels")}>
                  <IdList
                    kind="channel"
                    info={info}
                    ids={draft.deny_channels}
                    onChange={(next) => set("deny_channels", next)}
                    max={LIMITS.max_list}
                    name="Denied channels"
                    none="No channel is off limits."
                  />
                </Field>
              </div>
            )}
          </Panel>
        </div>

        <aside className="editor-rail">
          <PreviewPanel guild={guild} kind="responders" body={body} ready={ready} />
        </aside>
      </div>
      <SaveBar
        count={editor.changes}
        saving={editor.saving}
        blocked={nameMissing || unfinished !== null}
        error={
          editor.failure ??
          (nameMissing ? "Give it a name first" : unfinished ? "Finish the action first" : null)
        }
        onSave={onSave}
        onReset={editor.reset}
      />
    </div>
  );
}
