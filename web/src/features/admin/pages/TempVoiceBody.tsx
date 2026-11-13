import { t as deferredText } from "../../../i18n/dashboard";
import { useCallback, useState } from "react";
import { Link } from "react-router";
import { Field } from "../../../components/Field";
import { channelLabel } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { Toast } from "../../../components/Toast";
import { ApiError } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { type TempVoice as TempVoiceData, useSaveCreator } from "../api/tempVoice";
import { t as text, tf } from "../api/tempVoiceI18n";
import { CreatorRow } from "./TempVoiceCreator";
import { RoomRow } from "./TempVoiceRoom";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { tempVoiceSummary } from "./tempVoiceSummary";

const VOICE = 2;
const MEMBER_COMMANDS: { command: string; what: string }[] = [
  { command: "/voice lock", what: "keep new people out" },
  { command: "/voice unlock", what: "open the room again" },
  { command: "/voice limit", what: "set how many fit" },
  { command: "/voice rename", what: "pick a name" },
  { command: "/voice kick", what: "send someone out for good" },
  { command: "/voice invite", what: "let someone in, even when locked" },
  { command: "/voice transfer", what: "hand the room to someone in it" },
  { command: "/voice claim", what: "take a room whose owner left" },
  { command: "/voice hide", what: "hide it from everyone outside" },
  { command: "/voice panel", what: "the same, as buttons" },
];
export function TempVoiceBody({ guild, data }: { guild: string; data: TempVoiceData }) {
  const discord = useDiscord(guild);
  const save = useSaveCreator(guild);
  const [editing, setEditing] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const clearToast = useCallback(() => setToast(null), []);
  const taken = new Set(data.creators.map((c) => c.channel_id));
  const [picked, setPicked] = useState<string>("");
  const voices = (discord.data?.channels ?? []).filter((c) => c.type === VOICE);
  const refused = save.error instanceof ApiError && save.variables?.channel === picked ? save.error : null;

  const add = () => {
    if (!picked) return;
    save.mutate(
      { channel: picked, patch: {} },
      {
        onSuccess: () => {
          setEditing(picked);
          setPicked("");
          setToast(text("Creator added. Join it to try it out."));
        },
      },
    );
  };

  return (
    <>
      {!data.enabled && (
        <p className="notice" role="note">
          <strong>{text("Temp voice")}</strong>
          {text(" is switched off in ")}
          <Link to={`/g/${guild}/modules`}>{text("Modules")}</Link>
          {text(", so joining a creator does nothing until you switch it on. ")}
        </p>
      )}
      {data.missing && data.missing.length > 0 && (
        <p className="notice notice-heart" role="note">
          {text("Pinchora is missing ")}
          <strong>{data.missing.map(text).join(", ")}</strong>
          {text(" in this server, so rooms won&rsquo;t work properly yet. Give its role ")}
          {data.missing.length === 1 ? text("that permission") : text("those permissions")}
          {text(" in Server Settings. ")}
        </p>
      )}

      <SettingsRow
        rowId="temp-voice-body-creators"
        title={dt("Creators", {}, "CREATORS")}
        description={dt("Create rooms from voice channels.")}
        summary={tempVoiceSummary(data).creators}
        initialOpen
      >
        <p className="muted panel-lead">
          {text(
            "A creator is a voice channel people join to get a room. Rooms are made in the creator&rsquo;s category, named by its template, and the person who joined is moved into theirs. ",
          )}
        </p>
        {data.creators.length === 0 ? (
          <p className="empty-row muted">
            {text(
              "No creator channels yet. Pick a voice channel below and everyone who joins it gets a room. ",
            )}
          </p>
        ) : (
          <ul className="creator-list" aria-label={text("Creator channels")}>
            {data.creators.map((creator) => (
              <CreatorRow
                key={`${creator.channel_id}:${editing === creator.channel_id}`}
                guild={guild}
                creator={creator}
                info={discord.data}
                limits={data.limits}
                open={editing === creator.channel_id}
                onToggle={() => setEditing(editing === creator.channel_id ? null : creator.channel_id)}
                onSaved={() => setToast(text("Saved. The bot uses it from the next room."))}
              />
            ))}
          </ul>
        )}

        <div className="add-creator">
          <Field
            label={text("Add a creator")}
            htmlFor="creator-channel"
            error={refused ? refused.message : null}
            hint={
              data.creators.length >= data.limits.max_creators
                ? tf(deferredText("A server can have {count} creators. Remove one to add another."), {
                    count: data.limits.max_creators,
                  })
                : voices.length === 0 && discord.data
                  ? text("This server has no voice channels yet. Make one in Discord first.")
                  : undefined
            }
          >
            <select
              id="creator-channel"
              className="pixel-input pixel-select"
              value={picked}
              onChange={(event) => setPicked(event.target.value)}
              disabled={!discord.data || data.creators.length >= data.limits.max_creators}
              aria-invalid={refused ? true : undefined}
            >
              <option value="">{text("Pick a voice channel…")}</option>
              {voices.map((c) => (
                <option key={c.id} value={c.id} disabled={taken.has(c.id)}>
                  {channelLabel(c)}
                  {taken.has(c.id) ? text(" (already a creator)") : ""}
                </option>
              ))}
            </select>
          </Field>
          <PixelButton onClick={add} disabled={!picked || save.isPending}>
            {save.isPending && save.variables?.channel === picked ? text("Adding…") : text("Add creator")}
          </PixelButton>
        </div>
      </SettingsRow>

      <SettingsRow
        rowId="temp-voice-body-live-rooms"
        title={dt("row.Live rooms", {}, "LIVE ROOMS")}
        description={dt("See rooms currently open.")}
        summary={tempVoiceSummary(data).rooms}
      >
        <span className="live-count" role="status">
          <span className="live-dot" aria-hidden="true" />
          <span className="pixel-label muted" aria-hidden="true">
            {dt("{count} of {max} live rooms open", { count: data.rooms.length, max: data.limits.max_rooms })}
          </span>
          <span className="visually-hidden">
            {tf(deferredText("{open} of {max} rooms open"), {
              open: data.rooms.length,
              max: data.limits.max_rooms,
            })}
          </span>
        </span>
        <p className="muted panel-lead">
          {text(
            "Rooms open right now. Members run their own rooms with the commands below; this list only shows them and refreshes by itself. ",
          )}
        </p>
        {data.rooms.length === 0 ? (
          <p className="empty-row muted">
            {text("No rooms right now. One appears as soon as somebody joins a creator. ")}
          </p>
        ) : (
          <ul className="room-list" aria-label={text("Live rooms")}>
            {data.rooms.map((room) => (
              <RoomRow key={room.channel_id} room={room} info={discord.data} creators={data.creators} />
            ))}
          </ul>
        )}
      </SettingsRow>

      <SettingsRow
        rowId="temp-voice-body-what-members-can-do"
        title={dt("What members can do", {}, "WHAT MEMBERS CAN DO")}
        description={dt("Review owner controls.")}
        summary={tempVoiceSummary(data).controls}
      >
        <p className="muted panel-lead">
          {text("The owner of a room runs it with ")}
          <code>{text("/voice")}</code>
          {text(
            ", or with the button panel that appears in the room&rsquo;s chat. Managers can run any room they reach. ",
          )}
        </p>
        <ul className="command-list" aria-label={text("Member commands")}>
          {MEMBER_COMMANDS.map((c) => (
            <li key={c.command}>
              <code>{c.command}</code>
              <span className="muted">{text(c.what)}</span>
            </li>
          ))}
        </ul>
      </SettingsRow>
      <Toast message={toast} onDone={clearToast} />
    </>
  );
}
