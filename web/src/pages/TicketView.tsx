import { PixelLink } from "../components/PixelButton";
import { Panel } from "../components/Panel";
import {
  STATE_LABEL,
  type TicketDetail,
  type TranscriptAuthor,
  duration,
  personLabel,
  stars,
  transcriptUrl,
  useTicket,
  when,
} from "../lib/tickets";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

const STATE_BADGE = { open: "badge-gold", claimed: "badge-mint", closed: "tk-badge-quiet" } as const;

/** One ticket: what it was about, who handled it, and its transcript once it closed. */
export function TicketView({ guild, id }: { guild: string; id: string }) {
  const query = useTicket(guild, id);
  return (
    <div className="settings-page">
      <PixelLink variant="ghost" className="button-small tk-back" href={`/g/${guild}/tickets`}>
        ← All tickets
      </PixelLink>
      {query.isPending && <Loading label="Reading the ticket" />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && <TicketBody guild={guild} detail={query.data} />}
    </div>
  );
}

function TicketBody({ guild, detail }: { guild: string; detail: TicketDetail }) {
  const { ticket, transcript } = detail;
  const number = `#${String(ticket.number).padStart(4, "0")}`;
  const firstReply =
    ticket.first_reply_at === null
      ? null
      : (new Date(ticket.first_reply_at).getTime() - new Date(ticket.opened_at).getTime()) / 1000;
  return (
    <>
      <PageHead
        title={`TICKET ${number}`}
        lead={
          <>
            <span className={`badge ${STATE_BADGE[ticket.state]}`}>
              {STATE_LABEL[ticket.state].toUpperCase()}
            </span>{" "}
            {ticket.topic || "No topic"}
            {ticket.panel && <span className="muted"> · from the {ticket.panel} panel</span>}
          </>
        }
        action={
          transcript && (
            <a
              className="pixel-button pixel-button-secondary"
              href={transcriptUrl(guild, ticket.id)}
              download={`ticket-${String(ticket.number).padStart(4, "0")}.html`}
            >
              Download transcript
            </a>
          )
        }
      />
      <Panel title="DETAILS">
        <dl className="tk-facts">
          <div>
            <dt>Opened by</dt>
            <dd>{personLabel(ticket.opener)}</dd>
          </div>
          <div>
            <dt>Opened</dt>
            <dd>{when(ticket.opened_at)}</dd>
          </div>
          <div>
            <dt>Claimed by</dt>
            <dd>{personLabel(ticket.claimed_by)}</dd>
          </div>
          <div>
            <dt>First reply</dt>
            <dd>{firstReply === null ? "—" : `after ${duration(firstReply)}`}</dd>
          </div>
          <div>
            <dt>Closed by</dt>
            <dd>{ticket.closed_at ? (ticket.closed_by ? personLabel(ticket.closed_by) : "The bot") : "—"}</dd>
          </div>
          <div>
            <dt>Closed</dt>
            <dd>{when(ticket.closed_at)}</dd>
          </div>
          <div>
            <dt>Reason</dt>
            <dd>{ticket.close_reason || "—"}</dd>
          </div>
          <div>
            <dt>Rating</dt>
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
          <dl className="tk-answers" aria-label="Form answers">
            {ticket.answers.map((a, n) => (
              <div key={n}>
                <dt>{a.question}</dt>
                <dd>{a.answer}</dd>
              </div>
            ))}
          </dl>
        )}
      </Panel>
      <Panel
        title="TRANSCRIPT"
        aside={transcript && <span className="muted counter">{transcript.messages} messages</span>}
      >
        {!transcript ? (
          <p className="empty-row muted">
            {ticket.state === "closed"
              ? "No transcript was kept for this ticket."
              : "The transcript is kept when the ticket closes. In Discord, the Transcript button shows it so far."}
          </p>
        ) : (
          <>
            {!transcript.with_content && (
              <p className="notice" role="note">
                <span className="badge badge-gold">NEEDS MESSAGE CONTENT</span> Kept without the Message
                Content intent: who wrote, when, and the files, but not the words.
              </p>
            )}
            {transcript.lines.length === 0 ? (
              <p className="empty-row muted">Nobody wrote anything in this ticket.</p>
            ) : (
              <ol className="tk-transcript" aria-label={`Transcript of ticket ${number}`}>
                {transcript.lines.map((line, n) => {
                  const author: TranscriptAuthor | undefined = transcript.authors.find(
                    (a) => a.idx === line.a,
                  );
                  const name = author?.name ?? null;
                  return (
                    <li key={n} className="tk-line">
                      <div className={`tk-who ${name === null ? "is-gone" : author?.bot ? "is-bot" : ""}`}>
                        {name ?? "Deleted user"}
                        {author?.bot && <span className="sr-only"> (bot)</span>}
                        <time dateTime={line.t}>{when(line.t)}</time>
                      </div>
                      <div>
                        {line.c ? <p className="tk-text">{line.c}</p> : null}
                        {line.f && line.f.length > 0 && (
                          <p className="tk-files">
                            📎 {line.f.length === 1 ? "1 file" : `${line.f.length} files`}:{" "}
                            {line.f.join(", ")}
                          </p>
                        )}
                        {line.e ? (
                          <p className="tk-files">{line.e === 1 ? "1 embed" : `${line.e} embeds`}</p>
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
