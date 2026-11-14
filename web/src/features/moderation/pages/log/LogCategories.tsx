import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { ChannelPicker } from "../../../../components/DashboardPickers";
import { PixelToggle } from "../../../../components/PixelToggle";
import type { GuildInfo, LogSettings } from "../../../../lib/api";
import type { LogDraft } from "../../api/moderation";

type Props = {
  categories: LogSettings["categories"];
  draft: Pick<LogDraft, "category_channels" | "events">;
  set: (next: Partial<LogDraft>) => void;
  info: GuildInfo | undefined;
  on: boolean;
  fieldError: (field: string) => string | null;
};

export function LogCategories({ categories, draft, set, info, on, fieldError }: Props) {
  return (
    <Panel title={deferredText("WHAT IT REPORTS")}>
      <p className="panel-lead muted">
        {" "}
        {deferredText("Entries marked")} <span className="badge badge-dim">{spaTranslate("audit log")}</span>{" "}
        {deferredText(
          "come from Discord's audit log and say who did it; Pinchora needs the View Audit Log permission for them.",
        )}{" "}
      </p>
      {fieldError("category_channels") && (
        <p className="field-error" role="alert">
          {fieldError("category_channels")}
        </p>
      )}
      <div className={`log-matrix ${on ? "" : "is-muted"}`}>
        {categories.map((category) => (
          <section key={category.key} className="log-category" aria-label={spaTranslate(category.label)}>
            <header className="log-category-head">
              <h3 className="setting-name">{spaTranslate(category.label)}</h3>
              <ChannelPicker
                posting
                label={deferredText("Channel for {arg1}", { arg1: spaTranslate(category.label) })}
                info={info}
                value={draft.category_channels[category.key] ?? null}
                onChange={(id) =>
                  set({ category_channels: { ...draft.category_channels, [category.key]: id } })
                }
                none={deferredText("The log channel")}
              />
            </header>
            <ul className="log-events">
              {category.events.map((event) => (
                <li key={event.key} className="log-event">
                  <span className="log-event-name">
                    {spaTranslate(event.label)}
                    {event.audit_log && <span className="badge badge-dim">{spaTranslate("audit log")}</span>}
                  </span>
                  <PixelToggle
                    label={spaTranslate(event.label)}
                    checked={draft.events[event.key] ?? false}
                    onChange={(next) => set({ events: { ...draft.events, [event.key]: next } })}
                  />
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
      {!on && (
        <p className="notice">{deferredText("Pick a log channel above; until then nothing is reported.")}</p>
      )}
    </Panel>
  );
}
