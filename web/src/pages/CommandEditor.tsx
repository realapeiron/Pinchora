import { useMemo, useState } from "react";
import { useNavigate, useOutletContext, useParams } from "react-router";

import { ActionsEditor } from "../components/ActionsEditor";
import { Choice } from "../components/Choice";
import { EmbedEditor } from "../components/EmbedEditor";
import { Field, SwitchRow } from "../components/Field";
import { IdList } from "../components/IdList";
import { Panel } from "../components/Panel";
import { PixelLink } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { PreviewPanel } from "../components/PreviewPanel";
import { ResponsesEditor } from "../components/ResponsesEditor";
import { SaveBar } from "../components/SaveBar";
import type { CommandDef, Overview } from "../lib/api";
import { useDiscord, useSaveTag, useTag } from "../lib/queries";
import {
  LIMITS,
  actionsProblem,
  blankCommand,
  checkCommandName,
  defOf,
  embedIsBlank,
  finishedActions,
  placeholdersFor,
  tidyEmbed,
  tidyName,
} from "../lib/tags";
import { useTagEditor } from "../lib/useTagEditor";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

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
  if (commandId !== undefined && query.isPending) return <Loading label="Opening the command" />;
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
  const { draft, set } = editor;
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
        state: { toast: `Saved /tag ${tidyName(draft.name)}. The bot uses it from the next command.` },
      }),
    );
  };

  return (
    <div className="settings-page">
      <PageHead
        title={id ? "EDIT COMMAND" : "NEW COMMAND"}
        lead={
          <>
            Members run it with <code>/tag {tidyName(draft.name) || "name"}</code>.
          </>
        }
        action={
          <PixelLink variant="secondary" href={`/g/${guild}/commands`}>
            Back to the list
          </PixelLink>
        }
      />
      <div className="editor">
        <div className="editor-form">
          <Panel title="BASICS">
            <div className="form-grid form-grid-2">
              <Field
                label="Name"
                htmlFor="command-name"
                error={nameProblem}
                hint="What members type after /tag. Lowercase letters, numbers, - and _."
              >
                <div className="input-prefix">
                  <span className="input-prefix-text" aria-hidden="true">
                    /tag
                  </span>
                  <input
                    id="command-name"
                    className="pixel-input"
                    value={draft.name}
                    maxLength={LIMITS.name_max + 1}
                    autoComplete="off"
                    onChange={(event) => set("name", event.target.value)}
                    aria-invalid={nameProblem ? true : undefined}
                  />
                </div>
              </Field>
              <Field
                label="Description"
                htmlFor="command-description"
                error={editor.problem("description")}
                hint="Shown next to the name in suggestions. Optional."
                aside={
                  <span className="counter">
                    {[...draft.description].length}/{LIMITS.description_max}
                  </span>
                }
              >
                <input
                  id="command-description"
                  className="pixel-input"
                  value={draft.description}
                  maxLength={LIMITS.description_max}
                  onChange={(event) => set("description", event.target.value)}
                />
              </Field>
            </div>
            <Choice
              name="Who sees the answer"
              value={draft.reply_mode}
              options={[
                {
                  value: "public",
                  title: "Everyone in the channel",
                  detail: "The answer is a normal message.",
                },
                { value: "ephemeral", title: "Only the person who ran it", detail: "A private answer." },
              ]}
              onChange={(next) => set("reply_mode", next)}
            />
            <SwitchRow
              title="Switched on"
              control={
                <PixelToggle
                  label="Switched on"
                  checked={draft.enabled}
                  onChange={(next) => set("enabled", next)}
                />
              }
            >
              Off, members are told it is switched off and it leaves the suggestions.
            </SwitchRow>
          </Panel>

          <Panel title="REPLY">
            <ResponsesEditor
              responses={draft.responses}
              onChange={(next) => set("responses", next)}
              placeholders={placeholdersFor("command")}
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
              Off, people and roles you write in the text show as plain words and nobody is pinged. On, the
              ones you wrote can be pinged. What members type, and @everyone and @here, never can.
            </SwitchRow>
          </Panel>

          <Panel title="ACTIONS">
            <ActionsEditor
              actions={draft.actions}
              onChange={(next) => set("actions", next)}
              info={info}
              error={editor.problem("actions") ?? unfinished}
            />
          </Panel>

          <Panel title="WHO CAN USE IT, AND WHERE">
            <div className="form-grid form-grid-2">
              <Field
                label="Only these roles"
                error={editor.problem("allow_roles")}
                hint="Empty: everyone may."
              >
                <IdList
                  kind="role"
                  info={info}
                  ids={draft.allow_roles}
                  onChange={(next) => set("allow_roles", next)}
                  max={LIMITS.max_list}
                  name="Allowed roles"
                  none="Every role."
                />
              </Field>
              <Field
                label="Never these roles"
                error={editor.problem("deny_roles")}
                hint="A denied role wins."
              >
                <IdList
                  kind="role"
                  info={info}
                  ids={draft.deny_roles}
                  onChange={(next) => set("deny_roles", next)}
                  max={LIMITS.max_list}
                  name="Denied roles"
                  none="No role is turned away."
                />
              </Field>
              <Field
                label="Only in these channels"
                error={editor.problem("allow_channels")}
                hint="A category covers the channels under it. Empty: everywhere."
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
            <Field
              label="Cooldown"
              htmlFor="command-cooldown"
              error={editor.problem("cooldown_secs")}
              hint={`Seconds a member waits between uses, up to ${LIMITS.command_cooldown_max}. 0 is none.`}
            >
              <input
                id="command-cooldown"
                type="number"
                className="pixel-input input-short"
                min={0}
                max={LIMITS.command_cooldown_max}
                value={draft.cooldown_secs}
                onChange={(event) => set("cooldown_secs", Number(event.target.value))}
              />
            </Field>
          </Panel>
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
        error={editor.failure ?? (unfinished ? "Finish the action first" : null)}
        onSave={onSave}
        onReset={editor.reset}
      />
    </div>
  );
}
