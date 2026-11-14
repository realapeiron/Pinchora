import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { useMemo, useState } from "react";
import { useNavigate, useOutletContext, useParams } from "react-router";

import { ActionsEditor } from "../components/ActionsEditor";
import { TagReply } from "../components/TagReply";
import { Panel } from "../../../components/Panel";
import { PixelLink } from "../../../components/PixelButton";
import { PreviewPanel } from "../components/PreviewPanel";
import { SaveBar } from "../../../components/SaveBar";
import type { CommandDef, Overview } from "../../../lib/api";
import { useDiscord, useSaveTag, useTag } from "../../../lib/queries";
import {
  actionsProblem,
  blankCommand,
  checkCommandName,
  cooldownFor,
  defOf,
  embedIsBlank,
  finishedActions,
  tidyEmbed,
  tidyName,
} from "../../../lib/tags";
import { useTagEditor } from "../hooks/useTagEditor";
import { CommandBasics } from "./command/CommandBasics";
import { CommandAccess } from "./command/CommandAccess";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";

/** What is sent to the server: names and texts tidied, an empty embed left out. */
function tidyCommand(draft: CommandDef): CommandDef {
  return {
    ...draft,
    name: tidyName(draft.name),
    description: draft.description.trim(),
    responses: draft.responses.map((r) => r.trim()),
    embed: tidyEmbed(draft.embed),
  };
}

/** `/g/:guild/commands/new` and `/g/:guild/commands/:commandId`. */
export function CommandEditor() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const { commandId } = useParams();
  const query = useTag(guild, "commands", commandId ?? null);
  if (commandId !== undefined && query.isPending)
    return <Loading label={deferredText("Opening the command")} />;
  if (commandId !== undefined && query.isError) return <Problem error={query.error} />;
  return (
    <CommandForm
      key={commandId ?? "new"}
      guild={guild}
      id={query.data?.id}
      initial={query.data ? defOf(query.data) : blankCommand()}
    />
  );
}

function CommandForm({ guild, id, initial }: { guild: string; id?: string; initial: CommandDef }) {
  const navigate = useNavigate();
  const info = useDiscord(guild).data;
  const submit = useSaveTag(guild, "commands");
  const editor = useTagEditor(initial, tidyCommand, (def) => submit.mutateAsync({ id, def }));
  const { draft, set, patch } = editor;
  const [args, setArgs] = useState("");
  const [showName, setShowName] = useState(false);

  const nameCheck = draft.name.trim() !== "" || showName ? checkCommandName(draft.name) : null;
  const nameProblem = editor.problem("name") ?? (nameCheck && !nameCheck.ok ? nameCheck.message : null);
  const hasEmbed = draft.embed !== null && !embedIsBlank(draft.embed);
  const ready = draft.responses.some((r) => r.trim() !== "") || hasEmbed;
  const body = useMemo(
    () => ({
      command: {
        ...tidyCommand(draft),
        name: tidyName(draft.name) || "preview",
        actions: finishedActions(draft.actions),
      },
      args,
    }),
    [draft, args],
  );

  const unfinished = actionsProblem(draft.actions);
  const onSave = () => {
    setShowName(true);
    if (!checkCommandName(draft.name).ok || unfinished) return;
    void editor.save(() =>
      navigate(`/g/${guild}/commands`, {
        state: {
          toast: deferredText("Saved /tag {arg1}. The bot uses it from the next command.", {
            arg1: tidyName(draft.name),
          }),
        },
      }),
    );
  };

  return (
    <SettingsPage
      className="settings-page"
      title={id ? deferredText("EDIT COMMAND") : deferredText("NEW COMMAND")}
      lead={
        <>
          {" "}
          {deferredText("Members run it with")} <code>/tag {tidyName(draft.name) || "name"}</code>.
        </>
      }
      action={
        <PixelLink variant="secondary" href={`/g/${guild}/commands`}>
          {deferredText("Back to the list")}
        </PixelLink>
      }
    >
      <div className="editor">
        <div className="editor-form">
          <CommandBasics draft={draft} set={set} problem={editor.problem} nameProblem={nameProblem} />

          <TagReply
            draft={draft}
            set={set}
            problem={editor.problem}
            hasEmbed={hasEmbed}
            placeholderKind="command"
            mentionDescription={deferredText(
              "Off, people and roles you write in the text show as plain words and nobody is pinged. On, the ones you wrote can be pinged. What members type, and @everyone and @here, never can.",
            )}
          />

          <Panel title={spaTranslate("ACTIONS")}>
            <ActionsEditor
              actions={draft.actions}
              onChange={(next) =>
                patch({ actions: next, cooldown_secs: cooldownFor(next, draft.cooldown_secs) })
              }
              info={info}
              error={editor.problem("actions") ?? unfinished}
            />
          </Panel>

          <CommandAccess draft={draft} set={set} problem={editor.problem} info={info} />
        </div>

        <aside className="editor-rail">
          <PreviewPanel
            guild={guild}
            kind="commands"
            body={body}
            args={args}
            onArgs={setArgs}
            ready={ready}
          />
        </aside>
      </div>
      <SaveBar
        count={editor.changes}
        saving={editor.saving}
        blocked={(nameCheck !== null && !nameCheck.ok) || unfinished !== null}
        error={editor.failure ?? (unfinished ? deferredText("Finish the action first") : null)}
        onSave={onSave}
        onReset={editor.reset}
      />
    </SettingsPage>
  );
}
