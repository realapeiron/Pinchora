import { plural as spaPlural, translate as spaTranslate } from "../i18n";
import { useCallback, useState } from "react";
import { Link, useLocation, useOutletContext } from "react-router";

import { Panel } from "../components/Panel";
import { PixelButton, PixelLink } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { TagTransfer } from "../components/TagTransfer";
import { Toast } from "../components/Toast";
import type { AutoResponder, Overview } from "../lib/api";
import { useDeleteTag, useDiscord, useTags, useToggleTag } from "../lib/queries";
import { TRIGGERS, cooldownLabel } from "../lib/tags";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

/** What fires a responder, in a few words: "A member gets @VIP", "A message has “good morning”". */
function when(responder: AutoResponder, roleName: (id: string) => string): string {
  const label = spaTranslate(
    TRIGGERS.find((t) => t.key === responder.trigger_kind)?.label ?? responder.trigger_kind,
  );
  switch (responder.trigger_kind) {
    case "role_added":
    case "role_removed":
      return `${label}: ${roleName(responder.trigger_value ?? "")}`;
    case "keyword":
      return `${label}: “${responder.trigger_value ?? ""}”`;
    case "level_up":
      return responder.trigger_value ? `${label} to ${responder.trigger_value}+` : label;
    default:
      return label;
  }
}

/** Auto-responders: messages the bot sends by itself. */
export function Responders() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const list = useTags(guild, "responders");
  const info = useDiscord(guild).data;
  const location = useLocation();
  const [toast, setToast] = useState<string | null>(
    (location.state as { toast?: string } | null)?.toast ?? null,
  );
  const clearToast = useCallback(() => setToast(null), []);
  const toggle = useToggleTag(guild, "responders");
  const remove = useDeleteTag(guild, "responders");
  const [asking, setAsking] = useState<AutoResponder | null>(null);
  const roleName = (id: string) => {
    const role = info?.roles.find((r) => r.id === id);
    return role ? `@${role.name}` : spaTranslate("a deleted role");
  };
  const locked = list.data ? !list.data.message_content : false;
  const full = list.data ? list.data.items.length >= list.data.max : false;
  const keywords = list.data?.items.filter((r) => r.trigger_kind === "keyword") ?? [];

  return (
    <div className="settings-page">
      <PageHead
        title={spaTranslate("AUTO-RESPONDERS")}
        lead={spaTranslate(
          "Messages the bot sends by itself: a greeting when someone joins, a cheer on a level-up, an answer to a keyword.",
        )}
        action={
          <PixelLink
            href={`/g/${guild}/responders/new`}
            aria-disabled={full}
            className={full ? "is-disabled" : ""}
          >
            {" "}
            {spaTranslate("New responder")}{" "}
          </PixelLink>
        }
      />
      {locked && (
        <div className="notice" role="note" data-testid="keyword-lock">
          <strong>{spaTranslate("Keyword triggers are locked.")}</strong>{" "}
          {spaTranslate(
            "Hearing keywords means reading chat, which needs Discord's privileged Message Content intent. This bot runs without it",
          )}{" "}
          {keywords.length > 0
            ? spaPlural(keywords.length, {
                one: ", so your {count} keyword responder stays quiet",
                other: ", so your {count} keyword responders stay quiet",
              })
            : ""}{" "}
          {spaTranslate(". Joins, leaves, level-ups and role changes work as usual.")}{" "}
        </div>
      )}
      {list.isPending && <Loading label={spaTranslate("Reading the responders")} />}
      {list.isError && <Problem error={list.error} />}
      {list.isSuccess && (
        <Panel
          title={spaTranslate("RESPONDERS")}
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
              {" "}
              {spaTranslate(
                "No auto-responders yet. A welcome message for new members is a good first one.",
              )}{" "}
            </p>
          ) : (
            <ul className="tag-list">
              {list.data.items.map((responder) => {
                const asleep = responder.trigger_kind === "keyword" && locked;
                return (
                  <li key={responder.id} className="tag-row" data-responder={responder.name}>
                    <div className="tag-main">
                      <Link to={`/g/${guild}/responders/${responder.id}`} className="tag-name">
                        {responder.name}
                      </Link>
                      <p className="tag-detail muted">
                        {when(responder, roleName)}
                        {asleep && (
                          <span className="badge badge-gold tag-lock">{spaTranslate("LOCKED")}</span>
                        )}
                      </p>
                      <p className="tag-facts muted">
                        {responder.destination === "dm"
                          ? spaTranslate("Answers by direct message")
                          : responder.destination === "channel"
                            ? spaTranslate("Answers in one channel")
                            : spaTranslate("Answers where it happened")}
                        {responder.chance < 100 && ` · ${responder.chance}% of the time`}
                        {responder.cooldown_secs > 0 &&
                          spaTranslate(" · cooldown {arg1}", {
                            arg1: cooldownLabel(responder.cooldown_secs),
                          })}
                        {responder.actions.length > 0 &&
                          spaPlural(responder.actions.length, {
                            one: " · {count} action",
                            other: " · {count} actions",
                          })}
                      </p>
                    </div>
                    <div className="tag-actions">
                      <PixelToggle
                        label={spaTranslate("Responder {arg1}", { arg1: responder.name })}
                        checked={responder.enabled}
                        onChange={(enabled) => toggle.mutate({ id: responder.id, enabled })}
                        disabled={toggle.isPending || (asleep && !responder.enabled)}
                      />
                      <PixelLink
                        variant="secondary"
                        className="button-small"
                        href={`/g/${guild}/responders/${responder.id}`}
                      >
                        {spaTranslate("Edit")}
                      </PixelLink>
                      <PixelButton
                        variant="ghost"
                        className="button-small"
                        aria-label={spaTranslate("Delete {arg1}", { arg1: responder.name })}
                        onClick={() => setAsking(responder)}
                      >
                        {spaTranslate("Delete")}
                      </PixelButton>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
          {full && (
            <p className="field-hint muted">
              {spaTranslate("A server has at most")} {list.data.max} {spaTranslate("auto-responders.")}
            </p>
          )}
        </Panel>
      )}
      {list.isSuccess && <TagTransfer guild={guild} kind="responders" />}
      {asking && (
        <div className="dialog-backdrop" role="presentation">
          <div className="panel panel-gold dialog" role="alertdialog" aria-labelledby="delete-title">
            <div className="panel-body">
              <h2 id="delete-title" className="pixel-title">
                {" "}
                {spaTranslate("Delete “")}
                {asking.name}”?
              </h2>
              <p>
                {spaTranslate(
                  "The bot stops answering this way. This can't be undone, but you can export first.",
                )}
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
                        setToast(spaTranslate("Deleted “{arg1}”.", { arg1: asking.name }));
                        setAsking(null);
                      },
                    })
                  }
                >
                  {remove.isPending ? spaTranslate("Deleting…") : "Delete it"}
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
