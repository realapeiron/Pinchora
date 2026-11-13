import { t as deferredText } from "../../../i18n/dashboard";
import type { Dispatch, SetStateAction } from "react";
import { PixelButton } from "../../../components/PixelButton";
import type { GuildInfo } from "../../../lib/api";
import { catalogWords } from "../../admin";
import { type EventBody, useEvents } from "../api/events";
import { EventFormFields } from "./EventFormFields";

export type EventDraft = { id?: string; body: EventBody };

export function EventEditor({
  editing,
  setEditing,
  info,
  language,
  t,
  preview,
  setPreview,
  changeTime,
  setNotice,
  events,
  save,
}: {
  editing: EventDraft;
  setEditing: Dispatch<SetStateAction<EventDraft | null>>;
  info: GuildInfo | undefined;
  language: string;
  t: ReturnType<typeof catalogWords>;
  preview: boolean;
  setPreview: Dispatch<SetStateAction<boolean>>;
  changeTime: (field: "starts_at" | "ends_at", value: string) => void;
  setNotice: (value: string) => void;
  events: ReturnType<typeof useEvents>;
  save: () => void;
}) {
  return (
    <div
      className="ev-dialog-backdrop"
      role="presentation"
      onClick={(e) => e.target === e.currentTarget && setEditing(null)}
    >
      <div
        className="ev-dialog ev-editor"
        role="dialog"
        aria-modal="true"
        aria-label={editing.id ? deferredText("Edit event") : t("Create event")}
      >
        <header>
          <h2>{editing.id ? deferredText("Edit event") : t("Create event")}</h2>
          <PixelButton variant="ghost" onClick={() => setEditing(null)}>
            {t("Close")}
          </PixelButton>
        </header>
        <div className="ev-form">
          <EventFormFields {...{ editing, setEditing, info, t, changeTime, setNotice }} />
          {preview && (
            <div className="ev-preview settings-preview">
              <strong>{editing.body.title || deferredText("Event title")}</strong>
              <p>{editing.body.description || t("Description")}</p>
              <span>
                {new Date(editing.body.starts_at).toLocaleString(language)} · {t("Going")} ·{" "}
                {deferredText("Maybe")} · {t("Declined")}
              </span>
            </div>
          )}
          <div className="ev-form-actions">
            <PixelButton variant="secondary" onClick={() => setPreview(!preview)}>
              {preview ? deferredText("Hide preview") : t("Preview")}
            </PixelButton>
            <PixelButton
              disabled={
                !editing.body.title.trim() ||
                !editing.body.channel_id ||
                events.create.isPending ||
                events.save.isPending
              }
              onClick={save}
            >
              {editing.id ? t("Save changes") : t("Create and post")}
            </PixelButton>
          </div>
        </div>
      </div>
    </div>
  );
}
