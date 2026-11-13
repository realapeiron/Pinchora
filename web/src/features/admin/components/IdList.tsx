import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { useState } from "react";

import type { GuildInfo } from "../../../lib/api";
import { ChannelPicker, ChannelTag, RolePicker, RoleTag } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";

/** A list of roles or channels as chips, with a picker to add one. */
export function IdList({
  kind,
  info,
  ids,
  onChange,
  max,
  name,
  none,
}: {
  kind: "role" | "channel";
  info: GuildInfo | undefined;
  ids: string[];
  onChange: (next: string[]) => void;
  max: number;
  /** What the list is, for screen readers: "Allowed roles". */
  name: string;
  none: string;
}) {
  const [pick, setPick] = useState<string | null>(null);
  const full = ids.length >= max;
  const nameOf = (id: string) =>
    kind === "role"
      ? (info?.roles.find((r) => r.id === id)?.name ?? id)
      : (info?.channels.find((c) => c.id === id)?.name ?? id);
  return (
    <div className="id-list">
      {ids.length === 0 ? (
        <p className="muted id-list-empty">{none}</p>
      ) : (
        <ul className="channel-chips" aria-label={name}>
          {ids.map((id) => (
            <li key={id} className="channel-chip">
              {kind === "role" ? <RoleTag info={info} id={id} /> : <ChannelTag info={info} id={id} />}
              <button
                type="button"
                className="chip-remove"
                aria-label={spaTranslate("Remove {arg1}", { arg1: nameOf(id) })}
                onClick={() => onChange(ids.filter((x) => x !== id))}
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="module-add">
        {kind === "role" ? (
          <RolePicker
            info={info}
            value={pick}
            onChange={setPick}
            none={full ? `Up to ${max}` : spaTranslate("Pick a role")}
            disabled={full}
          />
        ) : (
          <ChannelPicker
            label={deferredText("Add to {arg1}", { arg1: name })}
            info={info}
            value={pick}
            onChange={setPick}
            any
            none={full ? `Up to ${max}` : spaTranslate("Pick a channel")}
            disabled={full}
          />
        )}
        <PixelButton
          variant="secondary"
          className="button-small"
          disabled={!pick || full || ids.includes(pick)}
          onClick={() => {
            if (pick) onChange([...ids, pick]);
            setPick(null);
          }}
        >
          {" "}
          {spaTranslate("Add")}{" "}
        </PixelButton>
      </div>
    </div>
  );
}
