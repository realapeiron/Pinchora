import { useCallback, useState } from "react";
import { Link, useLocation, useOutletContext } from "react-router";

import { Panel } from "../components/Panel";
import { PixelButton, PixelLink } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { TagTransfer } from "../components/TagTransfer";
import { Toast } from "../components/Toast";
import type { CustomCommand, Overview } from "../lib/api";
import { useDeleteTag, useTags, useToggleTag } from "../lib/queries";
import { cooldownLabel, describeAction } from "../lib/tags";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

/** Custom commands: what members run with /tag. */
export function Commands() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const list = useTags(guild, "commands");
  const location = useLocation();
  const [toast, setToast] = useState<string | null>(
    (location.state as { toast?: string } | null)?.toast ?? null,
  );
  const clearToast = useCallback(() => setToast(null), []);
  const toggle = useToggleTag(guild, "commands");
  const remove = useDeleteTag(guild, "commands");
  const [asking, setAsking] = useState<CustomCommand | null>(null);
  const full = list.data ? list.data.items.length >= list.data.max : false;

  return (
    <div className="settings-page">
      <PageHead
        title="CUSTOM COMMANDS"
        lead={
          <>
            Commands your members run with <code>/tag</code>: a reply, an embed, a role to give. You can also
            write simple ones in Discord with <code>/setup commands</code>.
          </>
        }
        action={
          <PixelLink
            href={`/g/${guild}/commands/new`}
            aria-disabled={full}
            className={full ? "is-disabled" : ""}
          >
            New command
          </PixelLink>
        }
      />
      {list.isPending && <Loading label="Reading the commands" />}
      {list.isError && <Problem error={list.error} />}
      {list.isSuccess && (
        <Panel
          title="COMMANDS"
          aside={
            <span className="muted counter">
              {list.data.items.length}/{list.data.max}
            </span>
          }
        >
          {toggle.isError && (
            <p className="field-error" role="alert">
              {toggle.error.message}
            </p>
          )}
          {list.data.items.length === 0 ? (
            <p className="empty-row muted">
              No custom commands yet. Make the first one: a rules reminder, a link list, an FAQ answer.
            </p>
          ) : (
            <ul className="tag-list">
              {list.data.items.map((command) => (
                <li key={command.id} className="tag-row" data-command={command.name}>
                  <div className="tag-main">
                    <Link to={`/g/${guild}/commands/${command.id}`} className="tag-name">
                      /tag {command.name}
                    </Link>
                    {command.description && <p className="muted tag-detail">{command.description}</p>}
                    <p className="tag-facts muted">
                      Used {command.uses} {command.uses === 1 ? "time" : "times"}
                      {command.cooldown_secs > 0 && ` · cooldown ${cooldownLabel(command.cooldown_secs)}`}
                      {command.reply_mode === "ephemeral" && " · private answer"}
                      {command.embed && " · embed"}
                      {command.responses.length > 1 && ` · ${command.responses.length} replies`}
                      {command.actions.length > 0 &&
                        ` · ${command.actions.map((a) => describeAction(a)).join(", ")}`}
                      {command.allow_roles.length + command.allow_channels.length > 0 && " · limited"}
                    </p>
                  </div>
                  <div className="tag-actions">
                    <PixelToggle
                      label={`Command ${command.name}`}
                      checked={command.enabled}
                      onChange={(enabled) => toggle.mutate({ id: command.id, enabled })}
                      disabled={toggle.isPending}
                    />
                    <PixelLink
                      variant="secondary"
                      className="button-small"
                      href={`/g/${guild}/commands/${command.id}`}
                    >
                      Edit
                    </PixelLink>
                    <PixelButton
                      variant="ghost"
                      className="button-small"
                      aria-label={`Delete ${command.name}`}
                      onClick={() => setAsking(command)}
                    >
                      Delete
                    </PixelButton>
                  </div>
                </li>
              ))}
            </ul>
          )}
          {full && <p className="field-hint muted">A server has at most {list.data.max} custom commands.</p>}
        </Panel>
      )}
      {list.isSuccess && <TagTransfer guild={guild} kind="commands" />}
      {asking && (
        <div className="dialog-backdrop" role="presentation">
          <div className="panel panel-gold dialog" role="alertdialog" aria-labelledby="delete-title">
            <div className="panel-body">
              <h2 id="delete-title" className="pixel-title">
                Delete /tag {asking.name}?
              </h2>
              <p>
                Members can't run it any more. This can't be undone, but you can export your commands first.
              </p>
              {remove.isError && (
                <p className="field-error" role="alert">
                  {remove.error.message}
                </p>
              )}
              <div className="dialog-actions">
                <PixelButton
                  variant="secondary"
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
                        setToast(`Deleted /tag ${asking.name}.`);
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
      <Toast message={toast} onDone={clearToast} />
    </div>
  );
}
