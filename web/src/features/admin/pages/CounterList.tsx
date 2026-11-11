import { translate as spaTranslate } from "../../../i18n";
import { PixelButton } from "../../../components/PixelButton";
import { RichText } from "../../../components/RichText";
import { channelLabel } from "../../../components/Pickers";
import type { GuildInfo } from "../../../lib/api";
import { type Counter, type Counters, counterName } from "../api/stats";

const KIND_LABEL = (list: Counters | undefined, key: string) =>
  list?.kinds.find((k) => k.key === key)?.label ?? key;
function channelName(info: GuildInfo | undefined, counter: Counter) {
  const found = info?.channels.find((c) => c.id === counter.channel_id);
  if (found) return channelLabel(found);
  return counter.channel_name ? `#${counter.channel_name}` : "Deleted channel";
}
function relative(at: string | null): string {
  if (!at) return "never";
  const seconds = Math.max(0, (Date.now() - Date.parse(at)) / 1000);
  if (seconds < 60) return spaTranslate("just now");
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)} h ago`;
  return `${Math.floor(seconds / 86_400)} days ago`;
}
export function CounterList({
  list,
  counters,
  info,
  pending,
  onEdit,
  onRemove,
}: {
  list: Counters | undefined;
  counters: Counter[];
  info: GuildInfo | undefined;
  pending: boolean;
  onEdit: (counter: Counter) => void;
  onRemove: (id: string) => void;
}) {
  return (
    <>
      {list &&
        (counters.length === 0 ? (
          <p className="empty-row muted">{spaTranslate("No counter channels yet.")}</p>
        ) : (
          <ul className="counter-list" aria-label={spaTranslate("Counter channels")}>
            {counters.map((c) => (
              <li key={c.channel_id} className="counter-row">
                <div>
                  <p className="counter-name">
                    {c.name ??
                      (c.value !== null
                        ? counterName(c.template, c.value)
                        : c.template.replaceAll("{count}", "…"))}
                  </p>
                  <p className="counter-meta muted">
                    <span>{channelName(info, c)}</span>
                    <span>{KIND_LABEL(list, c.kind)}</span>
                    <span>
                      {c.name
                        ? spaTranslate("Renamed {arg1}", { arg1: relative(c.renamed_at) })
                        : spaTranslate("Not renamed yet")}
                    </span>
                  </p>
                </div>
                <div className="add-actions">
                  <PixelButton variant="ghost" className="button-small" onClick={() => onEdit(c)}>
                    {spaTranslate("Edit")}
                  </PixelButton>
                  <PixelButton
                    variant="ghost"
                    className="button-small"
                    aria-label={spaTranslate("Stop counting in {arg1}", {
                      arg1: channelName(info, c),
                    })}
                    disabled={pending}
                    onClick={() => onRemove(c.channel_id)}
                  >
                    {spaTranslate("Remove")}
                  </PixelButton>
                </div>
                {c.problem && (
                  <p className="counter-problem notice notice-heart" role="note">
                    <RichText text={c.problem} />
                  </p>
                )}
              </li>
            ))}
          </ul>
        ))}
    </>
  );
}
