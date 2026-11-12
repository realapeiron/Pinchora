import { useState } from "react";
import { translate as spaTranslate } from "../../../i18n";
import { Field } from "../../../components/Field";
import { ChannelPicker, channelLabel } from "../../../components/DashboardPickers";
import { PixelButton } from "../../../components/PixelButton";
import { ApiError, type Channel, type GuildInfo } from "../../../lib/api";
import {
  type Creator,
  type CreatorPatch,
  type TempVoice as TempVoiceData,
  MAX_TEMPLATE,
  roomName,
  templateProblem,
  useSaveCreator,
} from "../api/tempVoice";
import { t, tf } from "../api/tempVoiceI18n";
import { nameOf } from "./TempVoiceCreator";
import { RoomPreview } from "./TempVoiceRoomPreview";
import { CreatorSwitches } from "./TempVoiceCreatorSwitches";

const SAMPLE_OWNER = "Mira";
const CATEGORY = 4;
export function CreatorForm({
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

      <CreatorSwitches value={value} set={set} />

      <Field
        label={t("Private chat thread")}
        htmlFor={`${id}-thread`}
        error={serverError("panel_channel_id")}
        hint={t(
          "Optional. Pick a text channel and each room also gets a private thread there, with the panel in it. Members are added as they join.",
        )}
      >
        <ChannelPicker
          posting
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
        <RoomPreview
          value={value}
          rooms={creator.rooms}
          templateError={templateError}
          previewChat={previewChat}
        />
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
