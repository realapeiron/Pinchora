import { translate as spaTranslate } from "../i18n";
import { useCallback, useState } from "react";
import { Link, useOutletContext } from "react-router";

import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { ChannelPicker, channelLabel } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { Toast } from "../components/Toast";
import { ApiError, type Channel, type GuildInfo, type Overview } from "../lib/api";
import { useDiscord } from "../lib/queries";
import {
  type Creator,
  type CreatorPatch,
  type Room,
  type TempVoice as TempVoiceData,
  MAX_TEMPLATE,
  ago,
  roomName,
  templateProblem,
  useRemoveCreator,
  useSaveCreator,
  useTempVoice,
} from "../lib/tempVoice";
import { Loading, Problem } from "./States";
import { t, tf } from "../lib/tempVoiceI18n";
import { SettingsPage } from "./PageHead";

import "../styles/temp-voice.css";

/** What a member sees in a room that came from the sample owner, so a template can be read at a glance. */
const SAMPLE_OWNER = "Mira";
const VOICE = 2;
const CATEGORY = 4;

/** What members type, for the list at the bottom of the page. */
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

export function TempVoice() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const query = useTempVoice(guild);
  return (
    <SettingsPage
      className="settings-page temp-voice-page"
      title={t("TEMP VOICE")}
      lead={t(
        "Members join a creator channel and get a voice room of their own, with the keys. It goes away when the last person leaves.",
      )}
    >
      {query.isPending && <Loading label={t("Reading the creators")} />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && <Body guild={guild} data={query.data} />}
    </SettingsPage>
  );
}

function Body({ guild, data }: { guild: string; data: TempVoiceData }) {
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
          setToast(t("Creator added. Join it to try it out."));
        },
      },
    );
  };

  return (
    <>
      {!data.enabled && (
        <p className="notice" role="note">
          <strong>{t("Temp voice")}</strong>
          {t(" is switched off in ")}
          <Link to={`/g/${guild}/modules`}>{t("Modules")}</Link>
          {t(", so joining a creator does nothing until you switch it on. ")}
        </p>
      )}
      {data.missing && data.missing.length > 0 && (
        <p className="notice notice-heart" role="note">
          {t("Pinchora is missing ")}
          <strong>{data.missing.map(t).join(", ")}</strong>
          {t(" in this server, so rooms won&rsquo;t work properly yet. Give its role ")}
          {data.missing.length === 1 ? t("that permission") : t("those permissions")}
          {t(" in Server Settings. ")}
        </p>
      )}

      <Panel
        title={t("CREATORS")}
        aside={
          <span className="pixel-label muted">{`${data.creators.length}/${data.limits.max_creators}`}</span>
        }
      >
        <p className="muted panel-lead">
          {t(
            "A creator is a voice channel people join to get a room. Rooms are made in the creator&rsquo;s category, named by its template, and the person who joined is moved into theirs. ",
          )}
        </p>
        {data.creators.length === 0 ? (
          <p className="empty-row muted">
            {t("No creator channels yet. Pick a voice channel below and everyone who joins it gets a room. ")}
          </p>
        ) : (
          <ul className="creator-list" aria-label={t("Creator channels")}>
            {data.creators.map((creator) => (
              <CreatorRow
                key={creator.channel_id}
                guild={guild}
                creator={creator}
                info={discord.data}
                limits={data.limits}
                open={editing === creator.channel_id}
                onToggle={() => setEditing(editing === creator.channel_id ? null : creator.channel_id)}
                onSaved={() => setToast(t("Saved. The bot uses it from the next room."))}
              />
            ))}
          </ul>
        )}

        <div className="add-creator">
          <Field
            label={t("Add a creator")}
            htmlFor="creator-channel"
            error={refused ? refused.message : null}
            hint={
              data.creators.length >= data.limits.max_creators
                ? tf(spaTranslate("A server can have {count} creators. Remove one to add another."), {
                    count: data.limits.max_creators,
                  })
                : voices.length === 0 && discord.data
                  ? t("This server has no voice channels yet. Make one in Discord first.")
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
              <option value="">{t("Pick a voice channel…")}</option>
              {voices.map((c) => (
                <option key={c.id} value={c.id} disabled={taken.has(c.id)}>
                  {channelLabel(c)}
                  {taken.has(c.id) ? t(" (already a creator)") : ""}
                </option>
              ))}
            </select>
          </Field>
          <PixelButton onClick={add} disabled={!picked || save.isPending}>
            {save.isPending && save.variables?.channel === picked ? t("Adding…") : t("Add creator")}
          </PixelButton>
        </div>
      </Panel>

      <Panel
        title={t("LIVE ROOMS")}
        aside={
          <span className="live-count" role="status">
            <span className="live-dot" aria-hidden="true" />
            <span className="pixel-label muted" aria-hidden="true">
              {`${data.rooms.length}/${data.limits.max_rooms}`}
            </span>
            <span className="visually-hidden">
              {tf(spaTranslate("{open} of {max} rooms open"), {
                open: data.rooms.length,
                max: data.limits.max_rooms,
              })}
            </span>
          </span>
        }
      >
        <p className="muted panel-lead">
          {t(
            "Rooms open right now. Members run their own rooms with the commands below; this list only shows them and refreshes by itself. ",
          )}
        </p>
        {data.rooms.length === 0 ? (
          <p className="empty-row muted">
            {t("No rooms right now. One appears as soon as somebody joins a creator. ")}
          </p>
        ) : (
          <ul className="room-list" aria-label={t("Live rooms")}>
            {data.rooms.map((room) => (
              <RoomRow key={room.channel_id} room={room} info={discord.data} creators={data.creators} />
            ))}
          </ul>
        )}
      </Panel>

      <Panel title={t("WHAT MEMBERS CAN DO")}>
        <p className="muted panel-lead">
          {t("The owner of a room runs it with ")}
          <code>{t("/voice")}</code>
          {t(
            ", or with the button panel that appears in the room&rsquo;s chat. Managers can run any room they reach. ",
          )}
        </p>
        <ul className="command-list" aria-label={t("Member commands")}>
          {MEMBER_COMMANDS.map((c) => (
            <li key={c.command}>
              <code>{c.command}</code>
              <span className="muted">{t(c.what)}</span>
            </li>
          ))}
        </ul>
      </Panel>
      <Toast message={toast} onDone={clearToast} />
    </>
  );
}

// /Creators

/** A channel as the pickers name it, or what to say when Discord no longer has it. */
function nameOf(info: GuildInfo | undefined, id: string | null, gone: string): string {
  if (!id) return gone;
  const found = info?.channels.find((c) => c.id === id);
  return found ? found.name : info ? gone : "…";
}

/** What remounts a creator's form: the settings it edits, not the live count of rooms that refreshes. */
function formKey(c: Creator): string {
  return JSON.stringify([
    c.channel_id,
    c.category_id,
    c.name_template,
    c.default_limit,
    c.default_locked,
    c.max_per_user,
    c.panel,
    c.panel_channel_id,
    c.enabled,
  ]);
}

function CreatorRow({
  guild,
  creator,
  info,
  limits,
  open,
  onToggle,
  onSaved,
}: {
  guild: string;
  creator: Creator;
  info: GuildInfo | undefined;
  limits: TempVoiceData["limits"];
  open: boolean;
  onToggle: () => void;
  onSaved: () => void;
}) {
  const remove = useRemoveCreator(guild);
  const [sure, setSure] = useState(false);
  const name = nameOf(info, creator.channel_id, t("a deleted channel"));
  const gone = info !== undefined && !info.channels.some((c) => c.id === creator.channel_id);
  const category = creator.category_id ? nameOf(info, creator.category_id, t("a deleted category")) : null;
  return (
    <li className={`creator ${open ? "is-open" : ""} ${creator.enabled ? "" : "is-off"}`}>
      <div className="creator-head">
        <div className="creator-title">
          <span className="channel-tag" title={creator.channel_id}>
            🔊 {name}
          </span>
          <span className={`badge ${creator.enabled ? "badge-mint" : "badge-dim"}`}>
            {creator.enabled ? t("ON") : t("OFF")}
          </span>
          {gone && <span className="badge badge-heart">{t("CHANNEL DELETED")}</span>}
          {creator.default_locked && <span className="badge badge-dim">{t("STARTS LOCKED")}</span>}
        </div>
        <div className="creator-actions">
          <PixelButton
            variant="ghost"
            className="button-small"
            aria-expanded={open}
            aria-controls={open ? `creator-${creator.channel_id}-form` : undefined}
            aria-label={`${t(open ? spaTranslate("Close the settings of") : spaTranslate("Edit"))} ${name}`}
            onClick={onToggle}
          >
            {t(open ? spaTranslate("Close") : spaTranslate("Edit"))}
          </PixelButton>
          {sure ? (
            <>
              <PixelButton
                variant="secondary"
                className="button-small"
                aria-label={tf(spaTranslate("Really remove {name}"), { name })}
                disabled={remove.isPending}
                onClick={() => remove.mutate(creator.channel_id, { onSettled: () => setSure(false) })}
              >
                {t(remove.isPending ? spaTranslate("Removing…") : spaTranslate("Yes, remove"))}
              </PixelButton>
              <PixelButton variant="ghost" className="button-small" onClick={() => setSure(false)}>
                {t("Keep ")}
              </PixelButton>
            </>
          ) : (
            <PixelButton
              variant="ghost"
              className="button-small"
              aria-label={tf(spaTranslate("Remove {name}"), { name })}
              onClick={() => setSure(true)}
            >
              {t("Remove ")}
            </PixelButton>
          )}
        </div>
      </div>
      <p className="creator-summary muted">
        <span className="creator-sample">{roomName(creator.name_template, SAMPLE_OWNER, 1)}</span>
        {category ? tf(spaTranslate(" · in {category}"), { category }) : t(" · at the top of the list")}
        {creator.default_limit > 0
          ? tf(spaTranslate(" · {count} people"), { count: creator.default_limit })
          : t(" · no limit")}
        {creator.rooms > 0 ? tf(spaTranslate(" · {count} open now"), { count: creator.rooms }) : ""}
      </p>
      {remove.isError && (
        <p className="field-error" role="alert">
          {remove.error.message}
        </p>
      )}
      {open && (
        <CreatorForm
          key={formKey(creator)}
          guild={guild}
          creator={creator}
          info={info}
          limits={limits}
          onClose={onToggle}
          onSaved={onSaved}
        />
      )}
    </li>
  );
}

function CreatorForm({
  guild,
  creator,
  info,
  limits,
  onClose,
  onSaved,
}: {
  guild: string;
  creator: Creator;
  info: GuildInfo | undefined;
  limits: TempVoiceData["limits"];
  onClose: () => void;
  onSaved: () => void;
}) {
  const save = useSaveCreator(guild);
  const [draft, setDraft] = useState<CreatorPatch>({});
  const [preview, setPreview] = useState(false);
  const value = { ...creator, ...draft };
  const set = <K extends keyof CreatorPatch>(key: K, next: CreatorPatch[K]) =>
    setDraft((d) => {
      const out = { ...d, [key]: next };
      if (next === creator[key as keyof Creator]) delete out[key];
      return out;
    });
  const changed = Object.keys(draft).length;
  const templateError = templateProblem(value.name_template);
  const limit = Number.isNaN(value.default_limit) ? null : value.default_limit;
  const limitError =
    limit === null || !Number.isInteger(limit) || limit < 0 || limit > limits.max_limit
      ? tf(spaTranslate("A limit is 0 (no limit) to {max} people."), { max: limits.max_limit })
      : null;
  const refused = save.error instanceof ApiError ? save.error : null;
  const serverError = (field: string) => refused?.errors.find((e) => e.field === field)?.message ?? null;
  const id = `creator-${creator.channel_id}`;
  const categories = (info?.channels ?? []).filter((c) => c.type === CATEGORY);
  const blocked = templateError !== null || limitError !== null;
  const previewChat = value.panel
    ? value.panel_channel_id
      ? t("Controls appear in a private thread.")
      : t("Controls appear in the room chat.")
    : value.panel_channel_id
      ? t("A private chat thread is created. Members use /voice commands.")
      : t("Members use /voice commands.");

  const submit = () => {
    if (blocked || changed === 0) return;
    save.mutate({ channel: creator.channel_id, patch: draft }, { onSuccess: onSaved });
  };
  const insert = (word: string) => set("name_template", `${value.name_template}{${word}}`);

  return (
    <form
      id={`${id}-form`}
      className="creator-form"
      aria-label={tf(spaTranslate("Settings of {name}"), {
        name: nameOf(info, creator.channel_id, t("this creator")),
      })}
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <div className="form-grid form-grid-2">
        <Field
          label={t("Room name")}
          htmlFor={`${id}-template`}
          error={templateError ?? serverError("name_template")}
          hint={
            <>
              <code>{"{user}"}</code>
              {t("is the owner&rsquo;s name, ")}
              <code>{"{n}"}</code>
              {t("the room&rsquo;s number. ")}
            </>
          }
          aside={
            <span className={`counter ${value.name_template.length > MAX_TEMPLATE ? "is-over" : ""}`}>
              {value.name_template.length}/{MAX_TEMPLATE}
            </span>
          }
        >
          <input
            id={`${id}-template`}
            className="pixel-input"
            value={value.name_template}
            maxLength={MAX_TEMPLATE + 20}
            onChange={(event) => set("name_template", event.target.value)}
            aria-invalid={templateError || serverError("name_template") ? true : undefined}
          />
          <div className="chips" aria-label={t("Words to add to the name")}>
            <button type="button" className="chip" onClick={() => insert("user")}>
              {"{user}"}
            </button>
            <button type="button" className="chip" onClick={() => insert("n")}>
              {"{n}"}
            </button>
          </div>
        </Field>
        <Field
          label={t("Category for the rooms")}
          htmlFor={`${id}-category`}
          error={serverError("category_id")}
          hint={t("Rooms take on the category's permissions.")}
        >
          <select
            id={`${id}-category`}
            className="pixel-input pixel-select"
            value={value.category_id ?? ""}
            onChange={(event) => set("category_id", event.target.value || null)}
            disabled={!info}
          >
            <option value="">{t("No category (top of the list)")}</option>
            {value.category_id && !categories.some((c: Channel) => c.id === value.category_id) && (
              <option value={value.category_id}>
                {t("Deleted category (")}
                {value.category_id})
              </option>
            )}
            {categories.map((c) => (
              <option key={c.id} value={c.id}>
                {channelLabel(c)}
              </option>
            ))}
          </select>
        </Field>
        <Field
          label={t("People per room")}
          htmlFor={`${id}-limit`}
          error={limitError ?? serverError("default_limit")}
          hint={t("Where rooms start. 0 means no limit; owners can change it.")}
        >
          <input
            id={`${id}-limit`}
            className="pixel-input input-short"
            type="number"
            min={0}
            max={limits.max_limit}
            value={Number.isNaN(value.default_limit) ? "" : value.default_limit}
            onChange={(event) =>
              set("default_limit", event.target.value === "" ? Number.NaN : Number(event.target.value))
            }
            aria-invalid={limitError || serverError("default_limit") ? true : undefined}
          />
        </Field>
        <Field
          label={t("Rooms per member")}
          htmlFor={`${id}-per-user`}
          error={serverError("max_per_user")}
          hint={t("A member over this is taken back to their oldest room.")}
        >
          <select
            id={`${id}-per-user`}
            className="pixel-input pixel-select input-short"
            value={value.max_per_user}
            onChange={(event) => set("max_per_user", Number(event.target.value))}
          >
            {Array.from({ length: limits.max_per_user }, (_, n) => n + 1).map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </Field>
      </div>

      <div className="creator-switches">
        <SwitchRow
          title={t("Starts locked")}
          control={
            <PixelToggle
              label={t("Starts locked")}
              checked={value.default_locked}
              onChange={(on) => set("default_locked", on)}
            />
          }
        >
          {t("New rooms keep everyone out until the owner invites them or unlocks it. ")}
        </SwitchRow>
        <SwitchRow
          title={t("Button panel")}
          control={
            <PixelToggle
              label={t("Button panel")}
              checked={value.panel}
              onChange={(on) => set("panel", on)}
            />
          }
        >
          {t("Each room gets a message of buttons (lock, rename, kick…) in its own chat. ")}
        </SwitchRow>
        <SwitchRow
          title={t("Active")}
          control={
            <PixelToggle label={t("Active")} checked={value.enabled} onChange={(on) => set("enabled", on)} />
          }
        >
          {t("Off makes no new rooms; the ones that exist carry on and are cleaned up. ")}
        </SwitchRow>
      </div>

      <Field
        label={t("Private chat thread")}
        htmlFor={`${id}-thread`}
        error={serverError("panel_channel_id")}
        hint={t(
          "Optional. Pick a text channel and each room also gets a private thread there, with the panel in it. Members are added as they join.",
        )}
      >
        <ChannelPicker
          id={`${id}-thread`}
          info={info}
          value={value.panel_channel_id}
          onChange={(channel) => set("panel_channel_id", channel)}
          none={t("No thread: the room's own chat")}
        />
      </Field>

      <p className="sample-line">
        <span className="pixel-label muted">{t("LOOKS LIKE")}</span>
        <span className="creator-sample">
          🔊 {templateError ? "…" : roomName(value.name_template, SAMPLE_OWNER, creator.rooms + 1)}
        </span>
      </p>
      {preview && (
        <div className="creator-preview" role="region" aria-label={t("New room preview")}>
          <strong>
            🔊{" "}
            {templateError
              ? t("Fix the room name above")
              : roomName(value.name_template, SAMPLE_OWNER, creator.rooms + 1)}
          </strong>
          <span>
            {t(value.default_locked ? spaTranslate("Starts locked") : spaTranslate("Starts open"))} ·{" "}
            {value.default_limit
              ? tf(spaTranslate("{count} people"), { count: value.default_limit })
              : t("No people limit")}
          </span>
          <span>{previewChat}</span>
        </div>
      )}

      {refused && !refused.errors.length && (
        <p className="field-error" role="alert">
          {refused.message}
        </p>
      )}
      <div className="creator-foot">
        <PixelButton variant="ghost" onClick={() => setPreview((shown) => !shown)} aria-expanded={preview}>
          {t(preview ? spaTranslate("Hide preview") : spaTranslate("Preview room"))}
        </PixelButton>
        <PixelButton variant="ghost" onClick={onClose}>
          {t(changed > 0 ? spaTranslate("Discard") : spaTranslate("Close"))}
        </PixelButton>
        <PixelButton onClick={submit} disabled={blocked || changed === 0 || save.isPending}>
          {t(save.isPending ? spaTranslate("Saving…") : spaTranslate("Save creator"))}
        </PixelButton>
      </div>
    </form>
  );
}

// /Rooms

function RoomRow({ room, info, creators }: { room: Room; info: GuildInfo | undefined; creators: Creator[] }) {
  const name = nameOf(info, room.channel_id, t("Voice room"));
  const source = creators.find((c) => c.channel_id === room.source_id);
  const owner =
    room.owner_name ??
    (room.owner_id ? tf(spaTranslate("Member {id}"), { id: room.owner_id.slice(-4) }) : t("Nobody"));
  const full = room.user_limit > 0 ? `${room.members}/${room.user_limit}` : String(room.members);
  return (
    <li className="room">
      <span className="room-name channel-tag" title={room.channel_id}>
        🔊 {name}
      </span>
      <span className="room-owner">
        <span className="pixel-label muted">{t("OWNER")}</span> {owner}
      </span>
      <span className="room-people">
        <span className="pixel-label muted">{t("INSIDE")}</span>{" "}
        <span aria-label={tf(spaTranslate("{count} people inside"), { count: full })}>{full}</span>
      </span>
      <span className="room-flags">
        {room.locked && <span className="badge badge-gold">{t("LOCKED")}</span>}
        {room.hidden && <span className="badge badge-dim">{t("HIDDEN")}</span>}
        {!source && <span className="badge badge-dim">{t("CREATOR REMOVED")}</span>}
      </span>
      <span className="room-age muted">{ago(room.created_at)}</span>
    </li>
  );
}
