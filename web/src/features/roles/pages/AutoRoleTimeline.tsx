import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { RoleTag } from "../../../components/Pickers";
import type { GuildInfo } from "../../../lib/api";
import { type AutoKind, type AutoRole, spanText } from "../api/roles";

export const KINDS: { value: AutoKind; title: string; detail: string }[] = [
  { value: "join", title: "Members who join", detail: "People who come into the server." },
  { value: "bot", title: "Bots that join", detail: "Bots that get added to the server." },
  { value: "after_verify", title: "After the join gate", detail: "Members once they are let in." },
];
export const kindName = (kind: AutoKind) => spaTranslate(KINDS.find((k) => k.value === kind)?.title ?? kind);

/** The auto roles in the order a member meets them: what they get at once, and what comes later. */
export function Timeline({ items, info }: { items: AutoRole[]; info: GuildInfo | undefined }) {
  const kinds = KINDS.filter((k) => items.some((r) => r.kind === k.value && r.enabled));
  if (kinds.length === 0)
    return <p className="empty-row muted">{deferredText("Nobody gets a role on their own yet.")}</p>;
  return (
    <div className="stack">
      {kinds.map((k) => (
        <section key={k.value} aria-label={spaTranslate(k.title)}>
          <p className="pixel-label muted">{spaTranslate(k.title).toLocaleUpperCase()}</p>
          <ol className="timeline">
            {items
              .filter((r) => r.kind === k.value && r.enabled)
              .sort((a, b) => a.delay_secs - b.delay_secs)
              .map((r) => (
                <li key={r.role_id}>
                  <span className="timeline-when pixel-label">
                    {r.delay_secs > 0 ? `+ ${spanText(r.delay_secs)}` : deferredText("AT ONCE")}
                  </span>
                  <RoleTag info={info} id={r.role_id} />
                  {r.remove_after_secs > 0 && (
                    <span className="muted list-note">
                      {spaTranslate("taken back")} {spanText(r.remove_after_secs)} {spaTranslate("later")}
                    </span>
                  )}
                </li>
              ))}
          </ol>
        </section>
      ))}
    </div>
  );
}
