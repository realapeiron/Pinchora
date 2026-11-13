import { t as deferredText } from "../../../i18n/dashboard";
import { useState } from "react";
import { translate as spaTranslate } from "../../../i18n";
import { ChannelPicker, ChannelTag } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import type { GuildInfo } from "../../../lib/api";

export function ModuleChannels({
  info,
  max,
  channels,
  onChange,
}: {
  info: GuildInfo | undefined;
  max: number;
  channels: string[];
  onChange: (next: string[]) => void;
}) {
  const [pick, setPick] = useState<string | null>(null);
  const full = channels.length >= max;
  return (
    <div className="module-channels">
      {channels.length === 0 ? (
        <p className="muted">{deferredText("No channels yet.")}</p>
      ) : (
        <ul className="channel-chips" aria-label={spaTranslate("Channels")}>
          {channels.map((id) => (
            <li key={id} className="channel-chip">
              <ChannelTag info={info} id={id} />
              <button
                type="button"
                className="chip-remove"
                aria-label={spaTranslate("Remove {arg1}", {
                  arg1: info?.channels.find((c) => c.id === id)?.name ?? id,
                })}
                onClick={() => onChange(channels.filter((c) => c !== id))}
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="module-add">
        <ChannelPicker
          label={deferredText("Add a channel")}
          info={info}
          value={pick}
          onChange={setPick}
          any
          none={full ? `Up to ${max} channels` : deferredText("Pick a channel or category")}
          disabled={full}
        />
        <PixelButton
          variant="secondary"
          className="button-small"
          disabled={!pick || full || channels.includes(pick)}
          onClick={() => {
            if (pick) onChange([...channels, pick]);
            setPick(null);
          }}
        >
          {" "}
          {spaTranslate("Add")}{" "}
        </PixelButton>
      </div>
      <p className="field-hint muted">
        {" "}
        {deferredText("A category covers the channels and threads under it.")} {channels.length}{" "}
        {spaTranslate("of")} {max}.
      </p>
    </div>
  );
}
