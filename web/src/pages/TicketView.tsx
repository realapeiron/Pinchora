import { translate as spaTranslate } from "../i18n";
import { PixelButton, PixelLink } from "../components/PixelButton";
import { Panel } from "../components/Panel";
import {
  STATE_LABEL,
  type TicketDetail,
  type TranscriptAuthor,
  duration,
  personLabel,
  stars,
  transcriptUrl,
  useCloseTicket,
  useTicket,
  when,
} from "../lib/tickets";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";
import { TicketOperationsPanel } from "./TicketOperationsPanel";

const STATE_BADGE = { open: "badge-gold", claimed: "badge-mint", closed: "tk-badge-quiet" } as const;

/** One ticket: what it was about, who handled it, and its transcript once it closed. */
export function TicketView({ guild, id }: { guild: string; id: string }) {
  const query = useTicket(guild, id);
  return (
    <div className="settings-page">
      <PixelLink variant="ghost" className="button-small tk-back" href={`/g/${guild}/tickets`}>
        {" "}
        {spaTranslate("← All tickets")}{" "}
      </PixelLink>
      {query.isPending && <Loading label={spaTranslate("Reading the ticket")} />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && <TicketBody guild={guild} detail={query.data} />}
    </div>
  );
}

function TicketBody({ guild, detail }: { guild: string; detail: TicketDetail }) {
  const close = useCloseTicket(guild);
  const { ticket, transcript } = detail;
  const number = `#${String(ticket.number).padStart(4, "0")}`;
  const firstReply =
    ticket.first_reply_at === null
      ? null
      : (new Date(ticket.first_reply_at).getTime() - new Date(ticket.opened_at).getTime()) / 1000;
  return (
    <>
      <PageHead
        title={spaTranslate("TICKET {arg1}", { arg1: number })}
        lead={
          <>
            <span className={`badge ${STATE_BADGE[ticket.state]}`}>
              {spaTranslate(STATE_LABEL[ticket.state]).toLocaleUpperCase()}
            </span>{" "}
            {ticket.topic || "No topic"}
            {ticket.panel && (
              <span className="muted">
                {" "}
                {spaTranslate("· from the")} {ticket.panel} {spaTranslate("panel")}
              </span>
            )}
          </>
        }
        action={
          <>
            {ticket.state !== "closed" && (
              <PixelButton
                variant="secondary"
                className="tk-close"
                disabled={close.isPending}
                onClick={() => close.mutate(ticket.id)}
              >
                {close.isPending ? spaTranslate("Closing…") : spaTranslate("Close ticket")}
              </PixelButton>
            )}
            {transcript && (
              <a
                className="pixel-button pixel-button-secondary"
                href={transcriptUrl(guild, ticket.id)}
                download={`ticket-${String(ticket.number).padStart(4, "0")}.html`}
              >
                {" "}
                {spaTranslate("Download transcript")}{" "}
              </a>
            )}
          </>
        }
      />
      {close.isError && <Problem error={close.error} />}
      <Panel title={spaTranslate("DETAILS")}>
        <dl className="tk-facts">
          <div>
            <dt>{spaTranslate("Opened by")}</dt>
            <dd>{personLabel(ticket.opener)}</dd>
          </div>
          <div>
            <dt>{spaTranslate("Opened")}</dt>
            <dd>{when(ticket.opened_at)}</dd>
          </div>
          <div>
            <dt>{spaTranslate("Claimed by")}</dt>
            <dd>{personLabel(ticket.claimed_by)}</dd>
          </div>
          <div>
            <dt>{spaTranslate("First reply")}</dt>
            <dd>
              {firstReply === null ? "—" : spaTranslate("after {arg1}", { arg1: duration(firstReply) })}
            </dd>
          </div>
          <div>
            <dt>{spaTranslate("Closed by")}</dt>
            <dd>
              {ticket.closed_at
                ? ticket.closed_by
                  ? personLabel(ticket.closed_by)
                  : spaTranslate("The bot")
                : "—"}
            </dd>
          </div>
          <div>
            <dt>{spaTranslate("Closed")}</dt>
            <dd>{when(ticket.closed_at)}</dd>
          </div>
          <div>
            <dt>{spaTranslate("Reason")}</dt>
            <dd>{ticket.close_reason || "—"}</dd>
          </div>
          <div>
            <dt>{spaTranslate("Rating")}</dt>
            <dd>
              {ticket.rating ? (
                <span className="tk-stars" role="img" aria-label={`${ticket.rating} of 5 stars`}>
                  {stars(ticket.rating)}
                </span>
              ) : (
                "—"
              )}
            </dd>
          </div>
        </dl>
        {ticket.answers.length > 0 && (
          <dl className="tk-answers" aria-label={spaTranslate("Form answers")}>
            {ticket.answers.map((a, n) => (
              <div key={n}>
                <dt>{a.question}</dt>
                <dd>{a.answer}</dd>
              </div>
            ))}
          </dl>
        )}
      </Panel>
      <TicketOperationsPanel guild={guild} ticket={ticket} />
      <Panel
        title={spaTranslate("TRANSCRIPT")}
        aside={
          transcript && (
            <span className="muted counter">
              {transcript.messages}
              {spaTranslate("messages")}
            </span>
          )
        }
      >
        {!transcript ? (
          <p className="empty-row muted">
            {ticket.state === "closed"
              ? spaTranslate("No transcript was kept for this ticket.")
              : spaTranslate(
                  "The transcript is kept when the ticket closes. In Discord, the Transcript button shows it so far.",
                )}
          </p>
        ) : (
          <>
            {!transcript.with_content && (
              <p className="notice" role="note">
                <span className="badge badge-gold">{spaTranslate("NEEDS MESSAGE CONTENT")}</span>{" "}
                {spaTranslate(
                  "Kept without the Message Content intent: who wrote, when, and the files, but not the words.",
                )}{" "}
              </p>
            )}
            {transcript.lines.length === 0 ? (
              <p className="empty-row muted">{spaTranslate("Nobody wrote anything in this ticket.")}</p>
            ) : (
              <ol
                className="tk-transcript"
                aria-label={spaTranslate("Transcript of ticket {arg1}", { arg1: number })}
              >
                {transcript.lines.map((line, n) => {
                  const author: TranscriptAuthor | undefined = transcript.authors.find(
                    (a) => a.idx === line.a,
                  );
                  const name = author?.name ?? null;
                  return (
                    <li key={n} className="tk-line">
                      <div className={`tk-who ${name === null ? "is-gone" : author?.bot ? "is-bot" : ""}`}>
                        {name ?? "Deleted user"}
                        {author?.bot && <span className="sr-only"> {spaTranslate("(bot)")}</span>}
                        <time dateTime={line.t}>{when(line.t)}</time>
                      </div>
                      <div>
                        {line.c ? <p className="tk-text">{line.c}</p> : null}
                        {line.f && line.f.length > 0 && (
                          <p className="tk-files">
                            📎 {line.f.length === 1 ? spaTranslate("1 file") : `${line.f.length} files`}:{" "}
                            {line.f.join(", ")}
                          </p>
                        )}
                        {line.e ? (
                          <p className="tk-files">
                            {line.e === 1 ? spaTranslate("1 embed") : `${line.e} embeds`}
                          </p>
                        ) : null}
                        {!line.c && !line.f?.length && !line.e && <p className="tk-text muted">—</p>}
                      </div>
                    </li>
                  );
                })}
              </ol>
            )}
          </>
        )}
      </Panel>
    </>
  );
}
