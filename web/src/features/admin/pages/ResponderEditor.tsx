import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { useMemo } from "react";
import { useNavigate, useOutletContext, useParams } from "react-router";

import { ActionsEditor } from "../components/ActionsEditor";
import { Choice } from "../../../components/Choice";
import { Field } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { ChannelPicker } from "../../../components/DashboardPickers";
import { PixelLink } from "../../../components/PixelButton";
import { PreviewPanel } from "../components/PreviewPanel";
import { SaveBar } from "../../../components/SaveBar";
import type { Overview, ResponderDef, TriggerKind } from "../../../lib/api";
import { useDiscord, useMe, useSaveTag, useTag } from "../../../lib/queries";
import {
  actionsProblem,
  blankResponder,
  cooldownFor,
  destinationsFor,
  embedIsBlank,
  finishedActions,
  responderDefOf,
  tidyEmbed,
} from "../../../lib/tags";
import { useTagEditor } from "../hooks/useTagEditor";
import { ResponderTrigger } from "./responder/ResponderTrigger";
import { ResponderFrequency } from "./responder/ResponderFrequency";
import { ResponderBasics } from "./responder/ResponderBasics";
import { TagReply } from "../components/TagReply";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";

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
  if (responderId !== undefined && query.isPending)
    return <Loading label={deferredText("Opening the responder")} />;
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
        state: {
          toast: deferredText("Saved “{arg1}”. The bot uses it from the next event.", {
            arg1: draft.name.trim(),
          }),
        },
      }),
    );
  };

  return (
    <SettingsPage
      className="settings-page"
      title={id ? deferredText("EDIT RESPONDER") : deferredText("NEW RESPONDER")}
      lead={deferredText("Sent by the bot by itself when the trigger happens.")}
      action={
        <PixelLink variant="secondary" href={`/g/${guild}/responders`}>
          {deferredText("Back to the list")}
        </PixelLink>
      }
    >
      <div className="editor">
        <div className="editor-form">
          <ResponderBasics draft={draft} set={set} problem={editor.problem} keywordLocked={keywordLocked} />

          <ResponderTrigger
            draft={draft}
            set={set}
            problem={editor.problem}
            info={info}
            keywordLocked={keywordLocked}
            pickTrigger={pickTrigger}
          />

          <TagReply
            draft={draft}
            set={set}
            problem={editor.problem}
            hasEmbed={hasEmbed}
            placeholderKind={draft.trigger_kind}
            mentionDescription={deferredText(
              "Off, people and roles you write in the text show as plain words and only the member it is about is pinged. @everyone and @here never are.",
            )}
          />

          <Panel title={deferredText("WHERE IT ANSWERS")}>
            <Choice
              name="Where it answers"
              value={draft.destination}
              options={destinationsFor(kind)}
              onChange={(next) => set("destination", next)}
            />
            {draft.destination === "channel" && (
              <Field
                label={spaTranslate("Channel")}
                htmlFor="responder-channel"
                error={editor.problem("channel_id")}
              >
                <ChannelPicker
                  posting
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

          <Panel title={spaTranslate("ACTIONS")}>
            <ActionsEditor
              actions={draft.actions}
              onChange={(next) =>
                patch({ actions: next, cooldown_secs: cooldownFor(next, draft.cooldown_secs) })
              }
              info={info}
              trigger={kind}
              error={editor.problem("actions") ?? unfinished}
            />
          </Panel>

          <ResponderFrequency draft={draft} set={set} problem={editor.problem} info={info} />
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
          (nameMissing
            ? deferredText("Give it a name first")
            : unfinished
              ? deferredText("Finish the action first")
              : null)
        }
        onSave={onSave}
        onReset={editor.reset}
      />
    </SettingsPage>
  );
}
