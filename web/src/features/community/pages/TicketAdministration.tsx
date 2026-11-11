import { useEffect, useState } from "react";
import { translate as spaTranslate } from "../../../i18n";
import { Panel } from "../../../components/Panel";
import { PixelButton } from "../../../components/PixelButton";
import { type GuildInfo } from "../../../lib/api";
import { type Panel as TicketPanel } from "../api/tickets";
import {
  operationSettingsBody,
  type OperationSettings,
  type PanelOperations,
  useSaveTicketOperationSettings,
  useSaveTicketPanelOperations,
  useTicketOperationSettings,
  useTicketLeaderboard,
  useTicketPanelOperations,
} from "../api/ticket-operations";
import { Problem } from "../../../components/States";
import { SupportHoursForm, SavedReplies } from "./TicketSupport";
import { PanelFields, PanelFaq } from "./TicketPanelOperationsFields";

export function TicketAdministration({
  guild,
  panels,
  info,
}: {
  guild: string;
  panels: TicketPanel[];
  info?: GuildInfo;
}) {
  return (
    <>
      <HoursAndSnippets guild={guild} info={info} />
      {panels.length > 0 && <PanelControls guild={guild} panels={panels} info={info} />}
    </>
  );
}

function HoursAndSnippets({ guild, info }: { guild: string; info?: GuildInfo }) {
  const query = useTicketOperationSettings(guild);
  const save = useSaveTicketOperationSettings(guild);
  const leaderboard = useTicketLeaderboard(guild);
  const [draft, setDraft] = useState<OperationSettings | null>(null);
  useEffect(() => {
    if (query.data) setDraft(query.data);
  }, [query.data]);
  const changed =
    !!draft &&
    !!query.data &&
    JSON.stringify(operationSettingsBody(draft)) !== JSON.stringify(operationSettingsBody(query.data));
  return (
    <Panel title={spaTranslate("SUPPORT HOURS & SAVED REPLIES")}>
      {(query.error || save.error) && <Problem error={query.error ?? save.error} />}
      {draft && (
        <SupportHoursForm draft={draft} setDraft={setDraft} info={info} save={save} changed={changed} />
      )}
      <SavedReplies guild={guild} />
      {leaderboard.data && leaderboard.data.items.length > 0 && (
        <div className="tk-operations-form">
          <h3>{spaTranslate("Staff leaderboard")}</h3>
          <ol className="tk-operations-notes">
            {leaderboard.data.items.map((staff) => (
              <li key={staff.user_id}>
                <strong>{staff.name ?? staff.user_id}</strong> · {staff.claimed} {spaTranslate("claimed")}
                {" · "}
                {staff.closed} {spaTranslate("closed")}
              </li>
            ))}
          </ol>
        </div>
      )}
    </Panel>
  );
}

function PanelControls({ guild, panels, info }: { guild: string; panels: TicketPanel[]; info?: GuildInfo }) {
  const [panel, setPanel] = useState(panels[0]?.id ?? "");
  const query = useTicketPanelOperations(guild, panel);
  const save = useSaveTicketPanelOperations(guild, panel);
  const [draft, setDraft] = useState<PanelOperations | null>(null);
  useEffect(() => {
    if (query.data) setDraft(query.data);
  }, [query.data]);
  const changed = !!draft && !!query.data && JSON.stringify(draft) !== JSON.stringify(query.data);
  return (
    <Panel title={spaTranslate("PANEL OPERATIONS")}>
      <label>
        {spaTranslate("Panel")}
        <select
          className="pixel-input pixel-select"
          value={panel}
          onChange={(event) => {
            setPanel(event.target.value);
            setDraft(null);
          }}
        >
          {panels.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </label>
      {(query.error || save.error) && <Problem error={query.error ?? save.error} />}
      {draft && (
        <form
          className="tk-operations-form"
          onSubmit={(event) => {
            event.preventDefault();
            if (changed) save.mutate(draft);
          }}
        >
          <PanelFields draft={draft} setDraft={setDraft} info={info} />
          <PanelFaq draft={draft} setDraft={setDraft} />
          <PixelButton type="submit" disabled={!changed || save.isPending}>
            {spaTranslate("Save panel operations")}
          </PixelButton>
        </form>
      )}
    </Panel>
  );
}
