import { t as deferredText } from "../../../i18n/dashboard";
import { useId, useState } from "react";
import { Link } from "react-router";
import { translate as spaTranslate } from "../../../i18n";
import { Field } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { PixelButton } from "../../../components/PixelButton";
import { Loading, Problem } from "../../../components/States";
import {
  STATE_LABEL,
  type TicketFilter,
  personLabel,
  stars,
  useCloseTicket,
  useTicketList,
  when,
} from "../api/tickets";

const STATES = [
  { key: "", label: "All" },
  { key: "open", label: "Open" },
  { key: "claimed", label: "Claimed" },
  { key: "closed", label: "Closed" },
] as const;

const STATE_BADGE = { open: "badge-gold", claimed: "badge-mint", closed: "tk-badge-quiet" } as const;

export function TicketList({ guild }: { guild: string }) {
  const close = useCloseTicket(guild);
  const [filter, setFilter] = useState<TicketFilter>({ state: "", q: "", user: "", staff: "" });
  const [typed, setTyped] = useState<TicketFilter>(filter);
  const list = useTicketList(guild, filter);
  const base = useId();
  const items = list.data?.pages.flatMap((p) => p.items) ?? [];
  const apply = (next: Partial<TicketFilter>) => {
    const merged = { ...typed, ...next };
    setTyped(merged);
    setFilter(merged);
  };
  return (
    <Panel title={deferredText("ALL TICKETS")}>
      <form
        className="tk-filters"
        role="search"
        aria-label={deferredText("Filter tickets")}
        onSubmit={(event) => {
          event.preventDefault();
          setFilter(typed);
        }}
      >
        <div className="tk-segment" role="group" aria-label={spaTranslate("Show")}>
          {STATES.map((s) => (
            <button
              key={s.key}
              type="button"
              aria-pressed={filter.state === s.key}
              onClick={() => apply({ state: s.key })}
            >
              {spaTranslate(s.label)}
            </button>
          ))}
        </div>
        <Field label={deferredText("Words or number")} htmlFor={`${base}-q`}>
          <input
            id={`${base}-q`}
            className="pixel-input"
            type="search"
            placeholder={deferredText("billing, 42…")}
            value={typed.q}
            onChange={(e) => setTyped({ ...typed, q: e.target.value })}
            onBlur={() => setFilter(typed)}
          />
        </Field>
        <Field label={deferredText("Member id")} htmlFor={`${base}-user`}>
          <input
            id={`${base}-user`}
            className="pixel-input"
            inputMode="numeric"
            placeholder={deferredText("Who opened it")}
            value={typed.user}
            onChange={(e) => setTyped({ ...typed, user: e.target.value.replace(/\D/g, "") })}
            onBlur={() => setFilter(typed)}
          />
        </Field>
        <Field label={deferredText("Staff id")} htmlFor={`${base}-staff`}>
          <input
            id={`${base}-staff`}
            className="pixel-input"
            inputMode="numeric"
            placeholder={deferredText("Who claimed or closed it")}
            value={typed.staff}
            onChange={(e) => setTyped({ ...typed, staff: e.target.value.replace(/\D/g, "") })}
            onBlur={() => setFilter(typed)}
          />
        </Field>
        <PixelButton type="submit" variant="secondary" className="tk-search">
          {" "}
          {spaTranslate("Search")}{" "}
        </PixelButton>
      </form>
      {list.isPending && <Loading label={deferredText("Reading the tickets")} />}
      {list.isError && <Problem error={list.error} />}
      {list.isSuccess && items.length === 0 && (
        <p className="empty-row muted">
          {filter.state || filter.q || filter.user || filter.staff
            ? deferredText("No ticket matches.")
            : deferredText("No tickets yet. Once a panel is posted, the tickets members open show up here.")}
        </p>
      )}
      {items.length > 0 && (
        <>
          {/* eslint-disable jsx-a11y/no-redundant-roles, jsx-a11y/no-interactive-element-to-noninteractive-role -- Mobile display:block removes native table semantics in some browsers. */}
          <table className="tk-table" role="table">
            <caption className="sr-only">{deferredText("Tickets, newest first")}</caption>
            <thead role="rowgroup">
              <tr role="row">
                <th role="columnheader" scope="col">
                  {" "}
                  {spaTranslate("Ticket")}{" "}
                </th>
                <th role="columnheader" scope="col">
                  {spaTranslate("State")}
                </th>
                <th role="columnheader" scope="col">
                  {" "}
                  {spaTranslate("Opened by")}{" "}
                </th>
                <th role="columnheader" scope="col">
                  {" "}
                  {spaTranslate("Staff")}{" "}
                </th>
                <th role="columnheader" scope="col">
                  {spaTranslate("Opened")}
                </th>
                <th role="columnheader" scope="col">
                  {spaTranslate("Closed")}
                </th>
                <th role="columnheader" scope="col">
                  {" "}
                  {spaTranslate("Rating")}{" "}
                </th>
                <th role="columnheader" scope="col">
                  {spaTranslate("Action")}
                </th>
              </tr>
            </thead>
            <tbody role="rowgroup">
              {items.map((t) => (
                <tr role="row" key={t.id} data-ticket={t.number}>
                  <td role="cell" data-label="Ticket">
                    <Link className="tk-row-link" to={`/g/${guild}/tickets/${t.id}`}>
                      <span className="tk-number">#{String(t.number).padStart(4, "0")}</span>{" "}
                      {t.topic || "No topic"}
                      {t.panel && <span className="tk-sub">{t.panel}</span>}
                    </Link>
                  </td>
                  <td role="cell" data-label="State">
                    <span className={`badge ${STATE_BADGE[t.state]}`}>
                      {spaTranslate(STATE_LABEL[t.state]).toLocaleUpperCase()}
                    </span>
                  </td>
                  <td role="cell" data-label="Opened by">
                    {personLabel(t.opener)}
                  </td>
                  <td role="cell" data-label="Staff">
                    {personLabel(t.claimed_by ?? t.closed_by)}
                  </td>
                  <td role="cell" data-label="Opened">
                    {when(t.opened_at)}
                  </td>
                  <td role="cell" data-label="Closed">
                    {when(t.closed_at)}
                  </td>
                  <td role="cell" data-label="Rating">
                    {t.rating ? (
                      <span className="tk-stars" role="img" aria-label={`${t.rating} of 5 stars`}>
                        {stars(t.rating)}
                      </span>
                    ) : (
                      "—"
                    )}
                  </td>
                  <td role="cell" data-label="Action">
                    {t.state !== "closed" && (
                      <PixelButton
                        variant="secondary"
                        className="button-small tk-close"
                        disabled={close.isPending}
                        onClick={() => close.mutate(t.id)}
                      >
                        {spaTranslate("Close")}
                      </PixelButton>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {/* eslint-enable jsx-a11y/no-redundant-roles, jsx-a11y/no-interactive-element-to-noninteractive-role */}
        </>
      )}
      {close.isError && <Problem error={close.error} />}
      {list.hasNextPage && (
        <div className="tk-more">
          <PixelButton
            variant="secondary"
            onClick={() => list.fetchNextPage()}
            disabled={list.isFetchingNextPage}
          >
            {list.isFetchingNextPage ? spaTranslate("Loading…") : deferredText("Older tickets")}
          </PixelButton>
        </div>
      )}
    </Panel>
  );
}
