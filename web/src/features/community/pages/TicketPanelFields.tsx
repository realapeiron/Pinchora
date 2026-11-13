import { t as deferredText } from "../../../i18n/dashboard";
import { useState } from "react";
import { translate as spaTranslate } from "../../../i18n";

import { RolePicker, RoleTag } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { RichText } from "../../../components/RichText";
import { type GuildInfo } from "../../../lib/api";
import { type PanelDraft } from "../api/tickets";

/** A `<select>` of the server's categories. */
export function CategoryPicker({
  id,
  info,
  value,
  onChange,
  none,
  invalid,
}: {
  id: string;
  info: GuildInfo | undefined;
  value: string | null;
  onChange: (id: string | null) => void;
  none: string;
  invalid?: boolean;
}) {
  const categories = (info?.channels ?? []).filter((c) => c.type === 4);
  const known = !value || categories.some((c) => c.id === value);
  return (
    <select
      id={id}
      className="pixel-input pixel-select"
      value={value ?? ""}
      onChange={(event) => onChange(event.target.value || null)}
      disabled={!info}
      aria-invalid={invalid || undefined}
    >
      <option value="">{none}</option>
      {!known && (
        <option value={value!}>
          {deferredText("Deleted category (")}
          {value})
        </option>
      )}
      {categories.map((c) => (
        <option key={c.id} value={c.id}>
          {`▸ ${c.name}`}
        </option>
      ))}
    </select>
  );
}

/** The panel's support roles as chips, with a labelled picker to add one. */
export function SupportRoles({
  id,
  info,
  ids,
  max,
  onChange,
}: {
  id: string;
  info: GuildInfo | undefined;
  ids: string[];
  max: number;
  onChange: (next: string[]) => void;
}) {
  const [pick, setPick] = useState<string | null>(null);
  const full = ids.length >= max;
  const nameOf = (role: string) => info?.roles.find((r) => r.id === role)?.name ?? role;
  return (
    <div className="id-list">
      {ids.length === 0 ? (
        <p className="muted id-list-empty">
          {deferredText("No support role: only managers see the tickets.")}
        </p>
      ) : (
        <ul className="channel-chips" aria-label={deferredText("Support roles")}>
          {ids.map((role) => (
            <li key={role} className="channel-chip">
              <RoleTag info={info} id={role} />
              <button
                type="button"
                className="chip-remove"
                aria-label={spaTranslate("Remove {arg1}", { arg1: nameOf(role) })}
                onClick={() => onChange(ids.filter((x) => x !== role))}
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="module-add">
        <RolePicker
          id={id}
          info={info}
          value={pick}
          onChange={setPick}
          none={full ? `Up to ${max}` : spaTranslate("Pick a role")}
          disabled={full}
        />
        <PixelButton
          variant="secondary"
          className="button-small"
          disabled={!pick || full || ids.includes(pick)}
          onClick={() => {
            if (pick) onChange([...ids, pick]);
            setPick(null);
          }}
        >
          {spaTranslate("Add role")}
        </PixelButton>
      </div>
    </div>
  );
}

/** The panel as Discord shows it: the bot, the embed, the buttons or the menu. */
export function PanelPreview({ draft, labelledBy }: { draft: PanelDraft; labelledBy?: string }) {
  const color = /^#?[0-9a-fA-F]{6}$/.test(draft.color.trim())
    ? `#${draft.color.trim().replace(/^#/, "")}`
    : "#ff6b4a";
  const topics = draft.topics.filter((t) => t.label.trim());
  return (
    <div className={`discord ${draft.enabled ? "" : "dc-off"}`} role="region" aria-labelledby={labelledBy}>
      <div className="discord-head">
        <span className="discord-avatar" aria-hidden="true" />
        <strong>Pinchora</strong>
        <span className="discord-tag">{spaTranslate("APP")}</span>
      </div>
      <div className="embed-card" style={{ borderLeftColor: color }}>
        <div className="embed-main">
          <p className="embed-title">
            {draft.title.trim() ? <RichText text={draft.title} /> : spaTranslate("Untitled panel")}
          </p>
          {draft.description.trim() && (
            <p className="embed-description">
              <RichText text={draft.description} />
            </p>
          )}
          <p className="embed-footer muted">{deferredText("Pick a topic to open a private ticket.")}</p>
        </div>
      </div>
      {draft.style === "buttons" ? (
        <div className="dc-buttons" aria-label={spaTranslate("Buttons")}>
          {topics.slice(0, 5).map((t, n) => (
            <span key={n} className={`dc-button is-${t.tone}`}>
              {t.emoji && <span aria-hidden="true">{t.emoji}</span>}
              {t.label}
            </span>
          ))}
        </div>
      ) : (
        <>
          <div className="dc-select" aria-hidden="true">
            <span>{deferredText("What do you need help with?")}</span>
            <span>▾</span>
          </div>
          <ul className="dc-options" aria-label={deferredText("Menu options")}>
            {topics.slice(0, 25).map((t, n) => (
              <li key={n}>
                <span>
                  {t.emoji && <span aria-hidden="true">{t.emoji} </span>}
                  {t.label}
                </span>
                {t.description && <small>{t.description}</small>}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
