import { t as deferredText } from "../../../i18n/dashboard";
import { plural as spaPlural, translate as spaTranslate } from "../../../i18n";

import { ChannelTag } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { type GuildInfo } from "../../../lib/api";
import { type Panel as TicketPanel, useDeletePanel, usePostPanel } from "../api/tickets";

interface TicketRowProps {
  panel: TicketPanel;
  editing: string | null;
  info: GuildInfo | undefined;
  setEditing: (id: string | null) => void;
  publish: (panel: TicketPanel) => void;
  post: ReturnType<typeof usePostPanel>;
  setAsking: (panel: TicketPanel) => void;
}
export function TicketPanelRow({
  panel,
  editing,
  info,
  setEditing,
  publish,
  post,
  setAsking,
}: TicketRowProps) {
  return (
    <li className={`tk-panel-row ${editing === panel.id ? "is-editing" : ""}`} data-panel={panel.name}>
      <div className="tk-panel-main">
        <span className="tk-panel-name">
          {panel.name}
          {panel.message_id ? (
            <span className="badge badge-mint">{spaTranslate("POSTED")}</span>
          ) : (
            <span className="badge tk-badge-quiet">{deferredText("NOT POSTED")}</span>
          )}
          {!panel.enabled && <span className="badge badge-gold">{spaTranslate("OFF")}</span>}
        </span>
        <p className="tk-panel-facts">
          <ChannelTag info={info} id={panel.channel_id} /> ·{" "}
          {panel.style === "buttons" ? spaTranslate("Buttons") : spaTranslate("Menu")} ·{" "}
          {spaPlural(panel.topics.length, { one: "{count} topic", other: "{count} topics" })}
          {panel.questions.length > 0 &&
            spaPlural(panel.questions.length, {
              one: " · form of {count} question",
              other: " · form of {count} questions",
            })}
          {panel.support_roles.length > 0 &&
            spaPlural(panel.support_roles.length, {
              one: " · {count} support role",
              other: " · {count} support roles",
            })}
        </p>
      </div>
      <div className="tk-panel-actions">
        <PixelButton
          variant="secondary"
          className="button-small"
          onClick={() => setEditing(editing === panel.id ? null : panel.id)}
          aria-expanded={editing === panel.id}
          aria-label={deferredText("Edit {arg1}", { arg1: panel.name })}
        >
          {editing === panel.id ? spaTranslate("Close") : spaTranslate("Edit")}
        </PixelButton>
        <PixelButton
          className="button-small"
          onClick={() => publish(panel)}
          disabled={post.isPending}
          aria-label={`${panel.message_id ? deferredText("Update the post of") : spaTranslate("Post")} ${panel.name}`}
        >
          {panel.message_id ? deferredText("Update post") : spaTranslate("Post")}
        </PixelButton>
        <PixelButton
          variant="ghost"
          className="button-small"
          onClick={() => setAsking(panel)}
          aria-label={deferredText("Delete {arg1}", { arg1: panel.name })}
        >
          {spaTranslate("Delete")}
        </PixelButton>
      </div>
    </li>
  );
}
interface DeleteTicketProps {
  asking: TicketPanel;
  remove: ReturnType<typeof useDeletePanel>;
  editing: string | null;
  setEditing: (id: string | null) => void;
  setAsking: (panel: TicketPanel | null) => void;
  onToast: (text: string) => void;
}
export function DeleteTicketPanel({
  asking,
  remove,
  editing,
  setEditing,
  setAsking,
  onToast,
}: DeleteTicketProps) {
  return (
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
            {" "}
            {deferredText("Delete “")}
            {asking.name}”?
          </h2>
          <p>
            {deferredText("The panel and its posted message go. Its tickets and their transcripts stay.")}
          </p>
          {remove.isError && (
            <p className="field-error" role="alert">
              {remove.error.message}
            </p>
          )}
          <div className="dialog-actions">
            <PixelButton
              /* eslint-disable-next-line jsx-a11y/no-autofocus -- Dialog dismissal receives focus on open. */
              autoFocus
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
                    onToast(deferredText("Deleted “{arg1}”.", { arg1: asking.name }));
                    if (editing === asking.id) setEditing(null);
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
  );
}
