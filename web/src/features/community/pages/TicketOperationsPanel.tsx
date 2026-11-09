import { useState } from "react";
import { translate as spaTranslate } from "../../../i18n";
import { Panel } from "../../../components/Panel";
import { PixelButton } from "../../../components/PixelButton";
import { useTicketNotes, useTicketOperationAction, useTicketOperations } from "../api/ticket-operations";
import { type Ticket, when } from "../api/tickets";
import { Problem } from "../../../components/States";

export function TicketOperationsPanel({ guild, ticket }: { guild: string; ticket: Ticket }) {
  const [replyText, setReplyText] = useState("");
  const claim = useTicketOperationAction(guild, ticket.id, "claim");
  const unclaim = useTicketOperationAction(guild, ticket.id, "unclaim");
  const reply = useTicketOperationAction(guild, ticket.id, "reply");
  const ops = useTicketOperations(guild, ticket.id);
  const active = ticket.state !== "closed";
  const failure = claim.error ?? unclaim.error ?? reply.error ?? ops.error;

  return (
    <Panel title={spaTranslate("STAFF OPERATIONS")}>
      {failure && <Problem error={failure} />}
      <div className="tk-operations-state">
        <span>
          {spaTranslate("Priority")}: <strong>{spaTranslate(ops.data?.priority ?? "normal")}</strong>
        </span>
        <span>
          {spaTranslate("Opened")}: {when(ticket.opened_at)}
        </span>
        {ops.data?.escalated_at && (
          <span>
            {spaTranslate("Escalated")}: {when(ops.data.escalated_at)}
          </span>
        )}
        {ops.data?.close_request_expires_at && (
          <span>
            {spaTranslate("Close requested; expires")}: {when(ops.data.close_request_expires_at)}
          </span>
        )}
        {ticket.closed_at && (
          <span>
            {spaTranslate("Closed")}: {when(ticket.closed_at)}
          </span>
        )}
      </div>
      {active && (
        <div className="tk-operations-actions">
          {ticket.claimed_by ? (
            <PixelButton
              variant="secondary"
              disabled={unclaim.isPending}
              onClick={() => unclaim.mutate(undefined)}
            >
              {spaTranslate("Unclaim")}
            </PixelButton>
          ) : (
            <PixelButton
              variant="secondary"
              disabled={claim.isPending}
              onClick={() => claim.mutate(undefined)}
            >
              {spaTranslate("Claim")}
            </PixelButton>
          )}
        </div>
      )}
      {active && (
        <form
          className="tk-operations-form"
          onSubmit={(event) => {
            event.preventDefault();
            reply.mutate(replyText.trim(), { onSuccess: () => setReplyText("") });
          }}
        >
          <label htmlFor="ticket-web-reply">{spaTranslate("Reply as Pinchora")}</label>
          <textarea
            id="ticket-web-reply"
            className="pixel-input"
            maxLength={2000}
            rows={4}
            value={replyText}
            onChange={(event) => setReplyText(event.target.value)}
          />
          <PixelButton type="submit" disabled={!replyText.trim() || reply.isPending}>
            {reply.isPending ? spaTranslate("Sending…") : spaTranslate("Send reply")}
          </PixelButton>
        </form>
      )}
      <PrivateNotes guild={guild} ticket={ticket.id} />
    </Panel>
  );
}

function PrivateNotes({ guild, ticket }: { guild: string; ticket: string }) {
  const [noteText, setNoteText] = useState("");
  const note = useTicketOperationAction(guild, ticket, "notes");
  const notes = useTicketNotes(guild, ticket);
  if (notes.error) return <Problem error={notes.error} />;
  return (
    <>
      {note.error && <Problem error={note.error} />}
      <form
        className="tk-operations-form"
        onSubmit={(event) => {
          event.preventDefault();
          note.mutate(noteText.trim(), { onSuccess: () => setNoteText("") });
        }}
      >
        <label htmlFor="ticket-staff-note">{spaTranslate("Private staff note")}</label>
        <textarea
          id="ticket-staff-note"
          className="pixel-input"
          maxLength={2000}
          rows={3}
          value={noteText}
          onChange={(event) => setNoteText(event.target.value)}
        />
        <p className="muted">
          {spaTranslate("Staff notes stay off the ticket channel and every opener transcript.")}
        </p>
        <PixelButton type="submit" variant="secondary" disabled={!noteText.trim() || note.isPending}>
          {spaTranslate("Save private note")}
        </PixelButton>
      </form>
      {notes.data && notes.data.items.length > 0 && (
        <ol className="tk-operations-notes">
          {notes.data.items.map((item) => (
            <li key={item.id}>
              <span className="muted">
                {when(item.created_at)} · {item.author_id}
              </span>
              <p>{item.body}</p>
            </li>
          ))}
        </ol>
      )}
    </>
  );
}
