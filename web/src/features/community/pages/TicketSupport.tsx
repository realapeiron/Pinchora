import { t as deferredText } from "../../../i18n/dashboard";
import { useState, type Dispatch, type SetStateAction } from "react";
import { translate as spaTranslate } from "../../../i18n";
import { PixelButton } from "../../../components/PixelButton";
import { type GuildInfo } from "../../../lib/api";
import { Problem } from "../../../components/States";
import {
  snippetChanged,
  operationSettingsBody,
  type OperationSettings,
  useDeleteTicketSnippet,
  useSaveTicketOperationSettings,
  useSaveTicketSnippet,
  useTicketSnippets,
} from "../api/ticket-operations";

const DAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const time = (minutes: number) =>
  `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
const minutes = (value: string) => {
  const [h = 0, m = 0] = value.split(":").map(Number);
  return h * 60 + m;
};

export type SettingsEdit = {
  draft: OperationSettings;
  setDraft: Dispatch<SetStateAction<OperationSettings | null>>;
};
function SupportWindows({ draft, setDraft }: SettingsEdit) {
  return (
    <div className="tk-operations-hours">
      <strong>{deferredText("Support windows")}</strong>
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
        {deferredText("Add window")}
      </PixelButton>
    </div>
  );
}

export function SupportHoursForm({
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
        {deferredText("Support timezone")}
        <input
          className="pixel-input"
          value={draft.support_timezone}
          onChange={(event) => setDraft({ ...draft, support_timezone: event.target.value })}
          placeholder={spaTranslate("Europe/Istanbul")}
        />
      </label>
      <SupportWindows draft={draft} setDraft={setDraft} />
      <label>
        {deferredText("Outside-hours message")}
        <textarea
          className="pixel-input"
          rows={2}
          maxLength={1000}
          value={draft.out_of_hours_message}
          onChange={(event) => setDraft({ ...draft, out_of_hours_message: event.target.value })}
        />
      </label>
      <label>
        {deferredText("Status board channel")}
        <select
          className="pixel-input pixel-select"
          value={draft.status_channel_id ?? ""}
          onChange={(event) => setDraft({ ...draft, status_channel_id: event.target.value || null })}
        >
          <option value="">{deferredText("No status board")}</option>
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
        {deferredText("Close request timeout (hours)")}
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
        {deferredText("Save operations")}
      </PixelButton>
    </form>
  );
}

export function SavedReplies({ guild }: { guild: string }) {
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
        <h3>{deferredText("Saved replies")}</h3>
        <p className="muted">{deferredText("Use {user}, {server}, {number} and {topic} in replies.")}</p>
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
            {deferredText("Reply name")}
            <input
              className="pixel-input"
              maxLength={50}
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <label>
            {deferredText("Reply text")}
            <textarea
              className="pixel-input"
              rows={3}
              maxLength={2000}
              value={body}
              onChange={(event) => setBody(event.target.value)}
            />
          </label>
          <PixelButton type="submit" disabled={!changed || saveSnippet.isPending}>
            {deferredText("Save reply")}
          </PixelButton>
        </form>
      </div>
    </>
  );
}
