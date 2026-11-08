import { plural as spaPlural, translate as spaTranslate } from "../i18n";
import "../styles/tickets.css";

import { useCallback, useId, useState } from "react";
import { Link, useOutletContext, useParams } from "react-router";

import { Choice } from "../components/Choice";
import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { ChannelPicker } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import { ApiError, type GuildInfo, type ModuleEntry, type Overview } from "../lib/api";
import { useDiscord } from "../lib/queries";
import {
  type Limits,
  STATE_LABEL,
  type Summary,
  type TicketFilter,
  type TicketSettings,
  type TicketsOverview,
  duration,
  exampleName,
  personLabel,
  stars,
  useCloseTicket,
  useSaveTicketSettings,
  useTicketList,
  useTickets,
  when,
} from "../lib/tickets";
import { SettingsPage } from "./PageHead";
import { Loading, Problem } from "./States";
import { CategoryPicker, TicketPanels } from "./TicketPanels";
import { TicketView } from "./TicketView";

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
          <TicketPanels
            guild={guild}
            panels={query.data.panels}
            limits={query.data.limits}
            info={discord.data}
            onToast={setToast}
          />
          <SettingsForm
            guild={guild}
            saved={query.data.settings}
            limits={query.data.limits}
            info={discord.data}
            onToast={setToast}
          />
          <TicketList guild={guild} />
        </>
      )}
      <Toast message={toast} onDone={clearToast} />
    </SettingsPage>
  );
}

// /Numbers

function SummaryPanel({ summary }: { summary: TicketsOverview["summary"] }) {
  const [span, setSpan] = useState<"week" | "month">("week");
  const numbers: Summary = summary[span];
  return (
    <Panel
      title={spaTranslate("AT A GLANCE")}
      aside={
        <div className="tk-segment is-tight" role="group" aria-label={spaTranslate("Period")}>
          {(
            [
              ["week", spaTranslate("7 days")],
              ["month", spaTranslate("30 days")],
            ] as const
          ).map(([key, label]) => (
            <button
              key={key}
              type="button"

              aria-pressed={span === key}
              onClick={() => setSpan(key)}
            >
              {label}
            </button>
          ))}
        </div>
      }
    >
      <ul className="tk-stats">
        <li className="tk-stat">
          <span className="tk-stat-name">{spaTranslate("Open now")}</span>
          <span className="tk-stat-value">{numbers.open_now}</span>
        </li>
        <li className="tk-stat">
          <span className="tk-stat-name">{spaTranslate("Opened")}</span>
          <span className="tk-stat-value">{numbers.opened}</span>
        </li>
        <li className="tk-stat">
          <span className="tk-stat-name">{spaTranslate("Closed")}</span>
          <span className="tk-stat-value">{numbers.closed}</span>
        </li>
        <li className="tk-stat">
          <span className="tk-stat-name">{spaTranslate("First reply")}</span>
          <span className="tk-stat-value">{duration(numbers.median_first_reply_secs)}</span>
          <span className="tk-stat-detail">{spaTranslate("median")}</span>
        </li>
        <li className="tk-stat">
          <span className="tk-stat-name">{spaTranslate("Rating")}</span>
          <span className="tk-stat-value">
            {numbers.rating_avg === null ? "—" : numbers.rating_avg.toFixed(1)}
            {numbers.rating_avg !== null && (
              <span className="tk-stars" aria-hidden="true">
                {" "}
                ★
              </span>
            )}
          </span>
          <span className="tk-stat-detail">
            {spaPlural(numbers.ratings, { one: "{count} rating", other: "{count} ratings" })}
          </span>
        </li>
      </ul>
    </Panel>
  );
}

// /Settings

const MODES = [
  {
    value: "auto" as const,
    title: "Automatic",
    detail: "Channels, then private threads once the server has 400 channels.",
  },
  {
    value: "channel" as const,
    title: "Channels",
    detail: "Each ticket is a channel in the ticket category.",
  },
  {
    value: "thread" as const,
    title: "Private threads",
    detail: "Each ticket is a private thread off the panel's channel.",
  },
];

function SettingsForm({
  guild,
  saved,
  limits,
  info,
  onToast,
}: {
  guild: string;
  saved: TicketSettings;
  limits: Limits;
  info: GuildInfo | undefined;
  onToast: (message: string) => void;
}) {
  const [value, setValue] = useState<TicketSettings>(saved);
  const save = useSaveTicketSettings(guild);
  const base = useId();
  const set = <K extends keyof TicketSettings>(field: K, next: TicketSettings[K]) =>
    setValue((v) => ({ ...v, [field]: next }));
  const changes = (Object.keys(saved) as (keyof TicketSettings)[]).filter(
    (k) => saved[k] !== value[k],
  ).length;
  const refused = save.error instanceof ApiError ? save.error.errors : [];
  const errors: Record<string, string> = {};
  for (const e of refused) errors[e.field] ??= e.message;
  const local: string[] = [];
  if (
    !Number.isInteger(value.auto_close_hours) ||
    value.auto_close_hours < 0 ||
    value.auto_close_hours > limits.auto_close_hours
  ) {
    errors.auto_close_hours ??= spaTranslate("0 (never) to {arg1} hours.", { arg1: limits.auto_close_hours });
    local.push("auto_close_hours");
  }
  if (
    !Number.isInteger(value.max_open_per_user) ||
    value.max_open_per_user < 1 ||
    value.max_open_per_user > limits.open_per_user
  ) {
    errors.max_open_per_user ??= `1 to ${limits.open_per_user} tickets.`;
    local.push("max_open_per_user");
  }
  const example = exampleName(value.naming);
  if (!example) {
    errors.naming ??= "The name needs letters or numbers, like `ticket-{number}`.";
    local.push("naming");
  }

  return (
    <>
      <Panel title={spaTranslate("SETTINGS")}>
        <div className="form-grid form-grid-2">
          <Field
            label={spaTranslate("Ticket category")}
            htmlFor={`${base}-open`}
            error={errors.open_category_id}
            hint={spaTranslate("Where ticket channels are made. A panel can pick its own.")}
          >
            <CategoryPicker
              id={`${base}-open`}
              info={info}
              value={value.open_category_id}
              onChange={(id) => set("open_category_id", id)}
              none={spaTranslate("No category (top of the list)")}
              invalid={!!errors.open_category_id}
            />
          </Field>
          <Field
            label={spaTranslate("Closed category")}
            htmlFor={`${base}-closed`}
            error={errors.closed_category_id}
            hint={spaTranslate("Closed tickets move here. None: they stay where they are.")}
          >
            <CategoryPicker
              id={`${base}-closed`}
              info={info}
              value={value.closed_category_id}
              onChange={(id) => set("closed_category_id", id)}
              none={spaTranslate("Don't move closed tickets")}
              invalid={!!errors.closed_category_id}
            />
          </Field>
          <Field
            label={spaTranslate("Log channel")}
            htmlFor={`${base}-log`}
            error={errors.log_channel_id}
            hint={spaTranslate("Opened, claimed and closed tickets, with each transcript.")}
          >
            <ChannelPicker
              id={`${base}-log`}
              info={info}
              value={value.log_channel_id}
              onChange={(id) => set("log_channel_id", id)}
              none={spaTranslate("No log")}
              invalid={!!errors.log_channel_id}
            />
          </Field>
          <Field
            label={spaTranslate("Channel name")}
            htmlFor={`${base}-naming`}
            error={errors.naming}
            hint={
              <span className="tk-sample">
                {"{number}"}, {"{user}"} {spaTranslate("and")} {"{topic}"} {spaTranslate("are filled in:")}{" "}
                <code>#{example || "…"}</code>
              </span>
            }
          >
            <input
              id={`${base}-naming`}
              className="pixel-input"
              value={value.naming}
              maxLength={limits.naming}
              spellCheck={false}
              onChange={(e) => set("naming", e.target.value)}
              aria-invalid={errors.naming ? true : undefined}
            />
          </Field>
          <Field
            label={spaTranslate("Auto-close after (hours)")}
            htmlFor={`${base}-auto`}
            error={errors.auto_close_hours}
            hint={
              value.auto_close_hours === 0
                ? spaTranslate("0: tickets never close by themselves.")
                : spaTranslate("A ticket nobody writes in for {arg1} h closes.", {
                    arg1: value.auto_close_hours,
                  })
            }
          >
            <input
              id={`${base}-auto`}
              type="number"
              min={0}
              max={limits.auto_close_hours}
              className="pixel-input input-short"
              value={Number.isNaN(value.auto_close_hours) ? "" : value.auto_close_hours}
              onChange={(e) => set("auto_close_hours", e.target.valueAsNumber)}
              aria-invalid={errors.auto_close_hours ? true : undefined}
            />
          </Field>
          <Field
            label={spaTranslate("Open tickets per member")}
            htmlFor={`${base}-max`}
            error={errors.max_open_per_user}
            hint={`1 to ${limits.open_per_user}. One ticket a minute per member either way.`}
          >
            <input
              id={`${base}-max`}
              type="number"
              min={1}
              max={limits.open_per_user}
              className="pixel-input input-short"
              value={Number.isNaN(value.max_open_per_user) ? "" : value.max_open_per_user}
              onChange={(e) => set("max_open_per_user", e.target.valueAsNumber)}
              aria-invalid={errors.max_open_per_user ? true : undefined}
            />
          </Field>
        </div>
        <Field label={spaTranslate("Tickets are")} htmlFor={`${base}-mode`}>
          <Choice
            name="Where tickets live"
            value={value.mode}
            onChange={(mode) => set("mode", mode)}
            options={MODES}
          />
        </Field>
        <SwitchRow
          title={spaTranslate("Transcript by direct message")}
          control={
            <PixelToggle
              label={spaTranslate("Transcript by direct message")}
              checked={value.dm_transcript}
              onChange={(on) => set("dm_transcript", on)}
            />
          }
        >
          {" "}
          {spaTranslate(
            "The member gets their ticket's transcript when it closes (unless they turned server DMs off).",
          )}{" "}
        </SwitchRow>
        <SwitchRow
          title={spaTranslate("Ratings")}
          control={
            <PixelToggle
              label={spaTranslate("Ratings")}
              checked={value.rating_enabled}
              onChange={(on) => set("rating_enabled", on)}
            />
          }
        >
          {" "}
          {spaTranslate(
            "Ask the member for one to five stars after a close; the numbers above show the average.",
          )}{" "}
        </SwitchRow>
      </Panel>
      <SaveBar
        count={changes}
        saving={save.isPending}
        blocked={local.length > 0}
        error={
          local.length > 0
            ? spaTranslate("Fix the marked fields first")
            : save.isError && refused.length === 0
              ? save.error.message
              : null
        }
        onSave={() =>
          save.mutate(value, {
            onSuccess: () => onToast(spaTranslate("Saved. New tickets use it right away.")),
          })
        }
        onReset={() => {
          setValue(saved);
          save.reset();
        }}
      />
    </>
  );
}

// /The tickets

const STATES = [
  { key: "", label: "All" },
  { key: "open", label: "Open" },
  { key: "claimed", label: "Claimed" },
  { key: "closed", label: "Closed" },
] as const;

const STATE_BADGE = { open: "badge-gold", claimed: "badge-mint", closed: "tk-badge-quiet" } as const;

function TicketList({ guild }: { guild: string }) {
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
    <Panel title={spaTranslate("ALL TICKETS")}>
      <form
        className="tk-filters"
        role="search"
        aria-label={spaTranslate("Filter tickets")}
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
        <Field label={spaTranslate("Words or number")} htmlFor={`${base}-q`}>
          <input
            id={`${base}-q`}
            className="pixel-input"
            type="search"
            placeholder={spaTranslate("billing, 42…")}
            value={typed.q}
            onChange={(e) => setTyped({ ...typed, q: e.target.value })}
            onBlur={() => setFilter(typed)}
          />
        </Field>
        <Field label={spaTranslate("Member id")} htmlFor={`${base}-user`}>
          <input
            id={`${base}-user`}
            className="pixel-input"
            inputMode="numeric"
            placeholder={spaTranslate("Who opened it")}
            value={typed.user}
            onChange={(e) => setTyped({ ...typed, user: e.target.value.replace(/\D/g, "") })}
            onBlur={() => setFilter(typed)}
          />
        </Field>
        <Field label={spaTranslate("Staff id")} htmlFor={`${base}-staff`}>
          <input
            id={`${base}-staff`}
            className="pixel-input"
            inputMode="numeric"
            placeholder={spaTranslate("Who claimed or closed it")}
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
      {list.isPending && <Loading label={spaTranslate("Reading the tickets")} />}
      {list.isError && <Problem error={list.error} />}
      {list.isSuccess && items.length === 0 && (
        <p className="empty-row muted">
          {filter.state || filter.q || filter.user || filter.staff
            ? spaTranslate("No ticket matches.")
            : spaTranslate("No tickets yet. Once a panel is posted, the tickets members open show up here.")}
        </p>
      )}
      {items.length > 0 && (
        <table className="tk-table" role="table">
          <caption className="sr-only">{spaTranslate("Tickets, newest first")}</caption>
          <thead role="rowgroup">
            <tr role="row">
              <th scope="col" role="columnheader">
                {" "}
                {spaTranslate("Ticket")}{" "}
              </th>
              <th scope="col" role="columnheader">
                {spaTranslate("State")}
              </th>
              <th scope="col" role="columnheader">
                {" "}
                {spaTranslate("Opened by")}{" "}
              </th>
              <th scope="col" role="columnheader">
                {" "}
                {spaTranslate("Staff")}{" "}
              </th>
              <th scope="col" role="columnheader">
                {spaTranslate("Opened")}
              </th>
              <th scope="col" role="columnheader">
                {spaTranslate("Closed")}
              </th>
              <th scope="col" role="columnheader">
                {" "}
                {spaTranslate("Rating")}{" "}
              </th>
              <th scope="col" role="columnheader">
                {spaTranslate("Action")}
              </th>
            </tr>
          </thead>
          <tbody role="rowgroup">
            {items.map((t) => (
              <tr key={t.id} role="row" data-ticket={t.number}>
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
      )}
      {close.isError && <Problem error={close.error} />}
      {list.hasNextPage && (
        <div className="tk-more">
          <PixelButton
            variant="secondary"
            onClick={() => list.fetchNextPage()}
            disabled={list.isFetchingNextPage}
          >
            {list.isFetchingNextPage ? spaTranslate("Loading…") : spaTranslate("Older tickets")}
          </PixelButton>
        </div>
      )}
    </Panel>
  );
}
