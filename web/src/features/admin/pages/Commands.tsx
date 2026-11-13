import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { useCallback, useState } from "react";
import { Link, useLocation, useOutletContext } from "react-router";

import { Panel } from "../../../components/Panel";
import { PixelButton, PixelLink } from "../../../components/PixelButton";
import { SettingsRow } from "../../../components/SettingsRow";
import { t } from "../../../i18n/dashboard";
import { commandItemSummary, commandListSummary } from "./tagSummary";
import { TagTransfer } from "../components/TagTransfer";
import { Toast } from "../../../components/Toast";
import type { CustomCommand, Overview } from "../../../lib/api";
import { useDeleteTag, useTags, useToggleTag } from "../../../lib/queries";
import { cooldownLabel, describeAction } from "../../../lib/tags";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";

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
    <SettingsPage
      className="settings-page"
      title={deferredText("CUSTOM COMMANDS")}
      lead={deferredText(
        "Members run these commands with {tag}: send a reply or embed, or give a role. Create simple commands in Discord with {setup}.",
        { tag: "/tag", setup: "/setup commands" },
      )}
      action={
        <PixelLink
          href={`/g/${guild}/commands/new`}
          aria-disabled={full}
          className={full ? "is-disabled" : ""}
        >
          {" "}
          {deferredText("New command")}{" "}
        </PixelLink>
      }
    >
      {list.isPending && <Loading label={deferredText("Reading the commands")} />}
      {list.isError && <Problem error={list.error} />}
      {list.isSuccess && (
        <Panel
          title={spaTranslate("COMMANDS")}
          aside={
            <span className="muted counter">
              {list.data.items.length}/{list.data.max}
            </span>
          }
        >
          <p className="muted fine">{commandListSummary(list.data.items)}</p>
          {toggle.isError && (
            <p className="field-error" role="alert">
              {toggle.error.message}
            </p>
          )}
          {list.data.items.length === 0 ? (
            <p className="empty-row muted">
              {" "}
              {deferredText(
                "No custom commands yet. Make the first one: a rules reminder, a link list, an FAQ answer.",
              )}{" "}
            </p>
          ) : (
            <div className="settings-detail-list" aria-label={spaTranslate("COMMANDS")}>
              {list.data.items.map((command) => (
                <div key={command.id} data-command={command.name}>
                  <SettingsRow
                    rowId={`command-${command.id}`}
                    title={`/tag ${command.name}`}
                    description={t("Edit the command reply and actions.")}
                    summary={commandItemSummary(command)}
                    enabled={command.enabled}
                    onToggle={(enabled) => toggle.mutate({ id: command.id, enabled })}
                    toggleLabel={deferredText("Command {arg1}", { arg1: command.name })}
                    toggleDisabled={toggle.isPending}
                  >
                    <div className="settings-detail-fields">
                      <Link to={`/g/${guild}/commands/${command.id}`} className="tag-name">
                        /tag {command.name}
                      </Link>
                      {command.description && <p className="muted tag-detail">{command.description}</p>}
                      <p className="tag-facts muted">
                        {deferredText("Used")} {command.uses} {command.uses === 1 ? "time" : "times"}
                        {command.cooldown_secs > 0 &&
                          deferredText(" · cooldown {arg1}", { arg1: cooldownLabel(command.cooldown_secs) })}
                        {command.reply_mode === "ephemeral" && " · private answer"}
                        {command.embed && " · embed"}
                        {command.responses.length > 1 && ` · ${command.responses.length} replies`}
                        {command.actions.length > 0 &&
                          ` · ${command.actions.map((a) => describeAction(a)).join(", ")}`}
                        {command.allow_roles.length + command.allow_channels.length > 0 && " · limited"}
                      </p>
                      <div className="tag-actions">
                        <PixelLink
                          variant="secondary"
                          className="button-small"
                          href={`/g/${guild}/commands/${command.id}`}
                        >
                          {spaTranslate("Edit")}
                        </PixelLink>
                        <PixelButton
                          variant="ghost"
                          className="button-small"
                          aria-label={deferredText("Delete {arg1}", { arg1: command.name })}
                          onClick={() => setAsking(command)}
                        >
                          {spaTranslate("Delete")}
                        </PixelButton>
                      </div>
                    </div>
                  </SettingsRow>
                </div>
              ))}
            </div>
          )}
          {full && (
            <p className="field-hint muted">
              {deferredText("A server has at most")} {list.data.max} {deferredText("custom commands.")}
            </p>
          )}
        </Panel>
      )}
      {list.isSuccess && <TagTransfer guild={guild} kind="commands" />}
      {asking && (
        <div className="dialog-backdrop" role="presentation">
          <div className="panel panel-gold dialog" role="alertdialog" aria-labelledby="delete-title">
            <div className="panel-body">
              <h2 id="delete-title" className="pixel-title">
                {" "}
                {deferredText("Delete /tag")} {asking.name}?
              </h2>
              <p>
                {" "}
                {deferredText(
                  "Members can't run it any more. This can't be undone, but you can export your commands first.",
                )}{" "}
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
                  {spaTranslate("Keep it")}
                </PixelButton>
                <PixelButton
                  disabled={remove.isPending}
                  onClick={() =>
                    remove.mutate(asking.id, {
                      onSuccess: () => {
                        setToast(deferredText("Deleted /tag {arg1}.", { arg1: asking.name }));
                        setAsking(null);
                      },
                    })
                  }
                >
                  {remove.isPending ? deferredText("Deleting…") : "Delete it"}
                </PixelButton>
              </div>
            </div>
          </div>
        </div>
      )}
      <Toast message={toast} onDone={clearToast} />
    </SettingsPage>
  );
}
