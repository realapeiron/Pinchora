import { useEffect, useState, type Dispatch, type SetStateAction } from "react";
import { translate as spaTranslate } from "../i18n";
import { Panel } from "../components/Panel";
import { PixelButton } from "../components/PixelButton";
import { type GuildInfo } from "../lib/api";
import { type Panel as TicketPanel } from "../lib/tickets";
import {
  snippetChanged,
  operationSettingsBody,
  type OperationSettings,
  type PanelOperations,
  useDeleteTicketSnippet,
  useSaveTicketOperationSettings,
  useSaveTicketPanelOperations,
  useSaveTicketSnippet,
  useTicketOperationSettings,
  useTicketLeaderboard,
  useTicketPanelOperations,
  useTicketSnippets,
} from "../lib/ticket-operations";
import { Problem } from "./States";

const DAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const time = (minutes: number) =>
  `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
const minutes = (value: string) => {
  const [h = 0, m = 0] = value.split(":").map(Number);
  return h * 60 + m;
};

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

type SettingsEdit = {
  draft: OperationSettings;
  setDraft: Dispatch<SetStateAction<OperationSettings | null>>;
};
type PanelEdit = { draft: PanelOperations; setDraft: Dispatch<SetStateAction<PanelOperations | null>> };

function SupportWindows({ draft, setDraft }: SettingsEdit) {
  return (
    <div className="tk-operations-hours">
      <strong>{spaTranslate("Support windows")}</strong>
      {draft.support_hours.map((window, index) => (
        <div className="tk-operations-window" key={index}>
          <select
            className="pixel-input pixel-select"
            aria-label={spaTranslate("Weekday")}
            value={window.day}
            onChange={(event) =>
              setDraft({
                ...draft,
                support_hours: draft.support_hours.map((w, i) =>
                  i === index ? { ...w, day: Number(event.target.value) } : w,
                ),
              })
            }
          >
            {DAYS.map((day, i) => (
              <option key={day} value={i + 1}>
                {spaTranslate(day)}
              </option>
            ))}
          </select>
          <input
            className="pixel-input"
            type="time"
            aria-label={spaTranslate("Starts")}
            value={time(window.start)}
            onChange={(event) =>
              setDraft({
                ...draft,
                support_hours: draft.support_hours.map((w, i) =>
                  i === index ? { ...w, start: minutes(event.target.value) } : w,
                ),
              })
            }
          />
          <input
            className="pixel-input"
            type="time"
            aria-label={spaTranslate("Ends")}
            value={time(window.end)}
            onChange={(event) =>
              setDraft({
                ...draft,
                support_hours: draft.support_hours.map((w, i) =>
                  i === index ? { ...w, end: minutes(event.target.value) } : w,
                ),
              })
            }
          />
          <PixelButton
            type="button"
            variant="ghost"
            onClick={() =>
              setDraft({ ...draft, support_hours: draft.support_hours.filter((_, i) => i !== index) })
            }
          >
            {spaTranslate("Remove")}
          </PixelButton>
        </div>
      ))}
      <PixelButton
        type="button"
        variant="secondary"
        disabled={draft.support_hours.length >= 21}
        onClick={() =>
          setDraft({
            ...draft,
            support_hours: [...draft.support_hours, { day: 1, start: 540, end: 1020 }],
          })
        }
      >
        {spaTranslate("Add window")}
      </PixelButton>
    </div>
  );
}

function SupportHoursForm({
  draft,
  setDraft,
  info,
  save,
  changed,
}: SettingsEdit & {
  info?: GuildInfo;
  save: ReturnType<typeof useSaveTicketOperationSettings>;
  changed: boolean;
}) {
  return (
    <form
      className="tk-operations-form"
      onSubmit={(event) => {
        event.preventDefault();
        if (changed) save.mutate(operationSettingsBody(draft));
      }}
    >
      <label>
        {spaTranslate("Support timezone")}
        <input
          className="pixel-input"
          value={draft.support_timezone}
          onChange={(event) => setDraft({ ...draft, support_timezone: event.target.value })}
          placeholder={spaTranslate("Europe/Istanbul")}
        />
      </label>
      <SupportWindows draft={draft} setDraft={setDraft} />
      <label>
        {spaTranslate("Outside-hours message")}
        <textarea
          className="pixel-input"
          rows={2}
          maxLength={1000}
          value={draft.out_of_hours_message}
          onChange={(event) => setDraft({ ...draft, out_of_hours_message: event.target.value })}
        />
      </label>
      <label>
        {spaTranslate("Status board channel")}
        <select
          className="pixel-input pixel-select"
          value={draft.status_channel_id ?? ""}
          onChange={(event) => setDraft({ ...draft, status_channel_id: event.target.value || null })}
        >
          <option value="">{spaTranslate("No status board")}</option>
          {(info?.channels ?? [])
            .filter((c) => c.type === 0)
            .map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
        </select>
      </label>
      <label>
        {spaTranslate("Close request timeout (hours)")}
        <input
          className="pixel-input"
          type="number"
          min={1}
          max={168}
          value={draft.close_timeout_hours}
          onChange={(event) => setDraft({ ...draft, close_timeout_hours: Number(event.target.value) })}
        />
      </label>
      <PixelButton type="submit" disabled={!changed || save.isPending}>
        {spaTranslate("Save operations")}
      </PixelButton>
    </form>
  );
}

function SavedReplies({ guild }: { guild: string }) {
  const snippets = useTicketSnippets(guild);
  const saveSnippet = useSaveTicketSnippet(guild);
  const deleteSnippet = useDeleteTicketSnippet(guild);
  const [name, setName] = useState("");
  const [body, setBody] = useState("");
  const changed = snippetChanged(snippets.data?.items ?? [], name, body);
  return (
    <>
      {(snippets.error || saveSnippet.error || deleteSnippet.error) && (
        <Problem error={snippets.error ?? saveSnippet.error ?? deleteSnippet.error} />
      )}
      <div className="tk-operations-form">
        <h3>{spaTranslate("Saved replies")}</h3>
        <p className="muted">{spaTranslate("Use {user}, {server}, {number} and {topic} in replies.")}</p>
        <ul className="tk-operations-notes">
          {snippets.data?.items.map((snippet) => (
            <li key={snippet.id}>
              <strong>{snippet.name}</strong> <span>{snippet.body}</span>
              <PixelButton type="button" variant="ghost" onClick={() => deleteSnippet.mutate(snippet.id)}>
                {spaTranslate("Delete")}
              </PixelButton>
            </li>
          ))}
        </ul>
        <form
          className="tk-operations-form"
          onSubmit={(event) => {
            event.preventDefault();
            if (!changed) return;
            saveSnippet.mutate(
              { name: name.trim(), body: body.trim() },
              {
                onSuccess: () => {
                  setName("");
                  setBody("");
                },
              },
            );
          }}
        >
          <label>
            {spaTranslate("Reply name")}
            <input
              className="pixel-input"
              maxLength={50}
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <label>
            {spaTranslate("Reply text")}
            <textarea
              className="pixel-input"
              rows={3}
              maxLength={2000}
              value={body}
              onChange={(event) => setBody(event.target.value)}
            />
          </label>
          <PixelButton type="submit" disabled={!changed || saveSnippet.isPending}>
            {spaTranslate("Save reply")}
          </PixelButton>
        </form>
      </div>
    </>
  );
}

function PanelFields({ draft, setDraft, info }: PanelEdit & { info?: GuildInfo }) {
  const numberField = (label: string, key: keyof PanelOperations, max: number) => (
    <label key={key}>
      {spaTranslate(label)}
      <input
        className="pixel-input"
        type="number"
        min={0}
        max={max}
        value={(draft?.[key] as number) ?? 0}
        onChange={(event) => draft && setDraft({ ...draft, [key]: Number(event.target.value) })}
      />
    </label>
  );
  return (
    <>
      <label>
        {spaTranslate("Claim mode")}
        <select
          className="pixel-input pixel-select"
          value={draft.claim_mode}
          onChange={(event) =>
            setDraft({ ...draft, claim_mode: event.target.value as PanelOperations["claim_mode"] })
          }
        >
          <option value="optional">{spaTranslate("Optional")}</option>
          <option value="required">{spaTranslate("Required")}</option>
          <option value="exclusive">{spaTranslate("Exclusive")}</option>
        </select>
      </label>
      <div className="tk-operations-grid">
        {numberField("Open tickets per panel (0 = unlimited)", "max_open_per_panel", 100)}
        {numberField("Open tickets per category (0 = unlimited)", "max_open_per_category", 100)}
        {numberField("Cooldown (seconds)", "cooldown_seconds", 86400)}
        {numberField("Minimum account age (days)", "minimum_account_age_days", 3650)}
        {numberField("Minimum membership (days)", "minimum_membership_days", 3650)}
        {numberField("Open claims per staff member (0 = unlimited)", "staff_open_limit", 100)}
      </div>
      <label>
        {spaTranslate("Overflow message")}
        <textarea
          className="pixel-input"
          rows={2}
          maxLength={1000}
          value={draft.overflow_message}
          onChange={(event) => setDraft({ ...draft, overflow_message: event.target.value })}
        />
      </label>
      <label>
        {spaTranslate("Bypass roles")}
        <select
          className="pixel-input pixel-select"
          multiple
          value={draft.bypass_roles}
          onChange={(event) =>
            setDraft({
              ...draft,
              bypass_roles: Array.from(event.target.selectedOptions, (option) => option.value),
            })
          }
        >
          {(info?.roles ?? []).map((role) => (
            <option key={role.id} value={role.id}>
              {role.name}
            </option>
          ))}
        </select>
      </label>
    </>
  );
}

function PanelFaq({ draft, setDraft }: PanelEdit) {
  return (
    <div className="tk-operations-form">
      <strong>{spaTranslate("FAQ before opening")}</strong>
      {draft.faq.map((faq, index) => (
        <div className="tk-operations-faq" key={index}>
          <input
            className="pixel-input"
            aria-label={spaTranslate("Question")}
            maxLength={150}
            value={faq.question}
            onChange={(event) =>
              setDraft({
                ...draft,
                faq: draft.faq.map((f, i) => (i === index ? { ...f, question: event.target.value } : f)),
              })
            }
          />
          <textarea
            className="pixel-input"
            aria-label={spaTranslate("Answer")}
            maxLength={1000}
            rows={2}
            value={faq.answer}
            onChange={(event) =>
              setDraft({
                ...draft,
                faq: draft.faq.map((f, i) => (i === index ? { ...f, answer: event.target.value } : f)),
              })
            }
          />
          <PixelButton
            type="button"
            variant="ghost"
            onClick={() => setDraft({ ...draft, faq: draft.faq.filter((_, i) => i !== index) })}
          >
            {spaTranslate("Remove")}
          </PixelButton>
        </div>
      ))}
      <PixelButton
        type="button"
        variant="secondary"
        disabled={draft.faq.length >= 10}
        onClick={() => setDraft({ ...draft, faq: [...draft.faq, { question: "", answer: "" }] })}
      >
        {spaTranslate("Add FAQ answer")}
      </PixelButton>
    </div>
  );
}
