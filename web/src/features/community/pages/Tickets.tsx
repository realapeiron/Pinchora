import { translate as spaTranslate } from "../../../i18n";
import "../../../styles/tickets.css";

import { useCallback, useState } from "react";
import { Link, useOutletContext, useParams } from "react-router";
import { Toast } from "../../../components/Toast";
import { SettingsRow } from "../../../components/SettingsRow";
import { t } from "../../../i18n/dashboard";
import { type ModuleEntry, type Overview } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { useTickets } from "../api/tickets";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import { TicketPanels } from "./TicketPanels";
import { TicketView } from "./TicketView";
import { TicketAdministration } from "./TicketAdministration";
import { SummaryPanel } from "./TicketNumbers";
import { SettingsForm } from "./TicketSettings";
import { TicketList } from "./TicketQueue";
import { ticketsSummary } from "./ticketsSummary";

/** Tickets: the numbers, the settings, the panels and the tickets; `/tickets/:ticketId` is one ticket. */
export function Tickets() {
  const data = useOutletContext<Overview>();
  const { ticketId } = useParams();
  const guild = data.guild.id;
  if (ticketId) return <TicketView guild={guild} id={ticketId} />;
  return <TicketsHome guild={guild} modules={data.modules} />;
}

function TicketsHome({ guild, modules }: { guild: string; modules: ModuleEntry[] }) {
  const query = useTickets(guild);
  const discord = useDiscord(guild);
  const [toast, setToast] = useState<string | null>(null);
  const clearToast = useCallback(() => setToast(null), []);
  const module = modules.find((m) => m.key === "tickets");
  return (
    <SettingsPage
      className="settings-page"
      title={spaTranslate("TICKETS")}
      lead={spaTranslate(
        "Members open a private ticket from a panel: a channel or thread just for them and your staff, closed with a transcript.",
      )}
    >
      {module && !module.enabled && (
        <p className="notice" role="note">
          <strong>{spaTranslate(module.label)}</strong> {spaTranslate("is switched off in")}{" "}
          <Link to={`/g/${guild}/modules`}>{spaTranslate("Modules")}</Link>
          {spaTranslate(", so panels don't open tickets until you switch it on.")}{" "}
        </p>
      )}
      {query.isPending && <Loading label={spaTranslate("Reading the tickets")} />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && (
        <>
          {!query.data.message_content && (
            <p className="notice" role="note" data-testid="content-lock">
              <span className="badge badge-gold">{spaTranslate("NEEDS MESSAGE CONTENT")}</span>{" "}
              {spaTranslate(
                "Transcripts keep who wrote, when, and the file names, but not the words: reading message text needs Discord's privileged Message Content intent, which this bot runs without.",
              )}{" "}
            </p>
          )}
          <SummaryPanel summary={query.data.summary} />
          <div className="settings-detail-list">
            <SettingsRow
              title={t("Ticket panels")}
              description={t("Create and post ticket entry panels.")}
              summary={ticketsSummary(query.data).panels}
              initialOpen
            >
              <TicketPanels
                guild={guild}
                panels={query.data.panels}
                limits={query.data.limits}
                info={discord.data}
                onToast={setToast}
              />
            </SettingsRow>
          </div>
          <SettingsForm
            guild={guild}
            saved={query.data.settings}
            limits={query.data.limits}
            info={discord.data}
            onToast={setToast}
          />
          <div className="settings-detail-list">
            <SettingsRow
              title={t("Support operations")}
              description={t("Manage support hours, saved replies and panel behavior.")}
              summary={
                query.data.panels.length ? ticketsSummary(query.data).panels : t("No ticket panels yet")
              }
            >
              <TicketAdministration guild={guild} panels={query.data.panels} info={discord.data} />
            </SettingsRow>
            <SettingsRow
              title={t("Ticket queue")}
              description={t("Search and manage member tickets.")}
              summary={ticketsSummary(query.data).queue}
            >
              <TicketList guild={guild} />
            </SettingsRow>
          </div>
        </>
      )}
      <Toast message={toast} onDone={clearToast} />
    </SettingsPage>
  );
}
