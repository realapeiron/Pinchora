import { useState } from "react";
import { Panel } from "../../../components/Panel";
import { PixelButton } from "../../../components/PixelButton";
import { RichText } from "../../../components/RichText";
import { translate as spaTranslate } from "../../../i18n";
import { type GuildInfo } from "../../../lib/api";
import {
  type Limits,
  type Panel as TicketPanel,
  draftOf,
  newPanel,
  useDeletePanel,
  usePostPanel,
} from "../api/tickets";
import { PanelEditor } from "./TicketPanelEditor";
import { DeleteTicketPanel, TicketPanelRow } from "./TicketPanelList";

export { CategoryPicker, PanelPreview } from "./TicketPanelFields";

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
      title={spaTranslate("PANELS")}
      aside={
        <span className="muted counter">
          {panels.length}/{limits.panels}
        </span>
      }
    >
      <p className="panel-lead muted">
        {" "}
        {spaTranslate("A panel is the message members open tickets from: up to")} {limits.buttons}{" "}
        {spaTranslate("topic buttons, or a menu of up to")} {limits.options}
        {spaTranslate(". Save it, then post it; posting again updates the message in place.")}{" "}
      </p>
      {post.isError && (
        <p className="field-error" role="alert">
          <RichText text={post.error.message} />
        </p>
      )}
      {panels.length === 0 ? (
        <p className="empty-row muted">
          {spaTranslate("No panels yet. Make one and post it where members ask for help.")}
        </p>
      ) : (
        <ul className="tk-panel-list">
          {panels.map((panel) => (
            <TicketPanelRow
              key={panel.id}
              {...{ panel, editing, info, setEditing, publish, post, setAsking }}
            />
          ))}
        </ul>
      )}
      <div className="panel-foot">
        {full && (
          <span className="muted fine">
            {spaTranslate("A server has at most")} {limits.panels} {spaTranslate("panels.")}
          </span>
        )}
        <PixelButton onClick={() => setEditing("new")} disabled={full || editing === "new"}>
          {" "}
          {spaTranslate("New panel")}{" "}
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
      {asking && <DeleteTicketPanel {...{ asking, remove, editing, setEditing, setAsking, onToast }} />}
    </Panel>
  );
}
