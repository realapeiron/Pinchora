import { useState } from "react";
import { translate as spaTranslate } from "../../../i18n";
import { PixelButton } from "../../../components/PixelButton";
import { type GuildInfo } from "../../../lib/api";
import { type Creator, type TempVoice as TempVoiceData, roomName, useRemoveCreator } from "../api/tempVoice";
import { t as text, tf } from "../api/tempVoiceI18n";
import { CreatorForm } from "./TempVoiceCreatorForm";
import { creatorSummary } from "./tempVoiceSummary";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";

const SAMPLE_OWNER = "Mira";
// /Creators

/** A channel as the pickers name it, or what to say when Discord no longer has it. */
export function nameOf(info: GuildInfo | undefined, id: string | null, gone: string): string {
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

export function CreatorRow({
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
  const name = nameOf(info, creator.channel_id, text("a deleted channel"));
  const gone = info !== undefined && !info.channels.some((c) => c.id === creator.channel_id);
  const category = creator.category_id ? nameOf(info, creator.category_id, text("a deleted category")) : null;
  return (
    <li className={`creator ${open ? "is-open" : ""} ${creator.enabled ? "" : "is-off"}`}>
      <SettingsRow
        title={`🔊 ${name}`}
        description={dt("Set this creator’s room defaults.")}
        summary={creatorSummary(creator)}
        initialOpen={open}
      >
        <div className="creator-head">
          <div className="creator-title">
            <span className={`badge ${creator.enabled ? "badge-mint" : "badge-dim"}`}>
              {creator.enabled ? text("ON") : text("OFF")}
            </span>
            {gone && <span className="badge badge-heart">{text("CHANNEL DELETED")}</span>}
            {creator.default_locked && <span className="badge badge-dim">{text("STARTS LOCKED")}</span>}
          </div>
          <div className="creator-actions">
            <PixelButton
              variant="ghost"
              className="button-small"
              aria-expanded={open}
              aria-controls={open ? `creator-${creator.channel_id}-form` : undefined}
              aria-label={`${text(open ? spaTranslate("Close the settings of") : spaTranslate("Edit"))} ${name}`}
              onClick={onToggle}
            >
              {text(open ? spaTranslate("Close") : spaTranslate("Edit"))}
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
                  {text(remove.isPending ? spaTranslate("Removing…") : spaTranslate("Yes, remove"))}
                </PixelButton>
                <PixelButton variant="ghost" className="button-small" onClick={() => setSure(false)}>
                  {text("Keep ")}
                </PixelButton>
              </>
            ) : (
              <PixelButton
                variant="ghost"
                className="button-small"
                aria-label={tf(spaTranslate("Remove {name}"), { name })}
                onClick={() => setSure(true)}
              >
                {text("Remove ")}
              </PixelButton>
            )}
          </div>
        </div>
        <p className="creator-summary muted">
          <span className="creator-sample">{roomName(creator.name_template, SAMPLE_OWNER, 1)}</span>
          {category ? tf(spaTranslate(" · in {category}"), { category }) : text(" · at the top of the list")}
          {creator.default_limit > 0
            ? tf(spaTranslate(" · {count} people"), { count: creator.default_limit })
            : text(" · no limit")}
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
      </SettingsRow>
    </li>
  );
}
