import { slug } from "./kitTemplates";
import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as t } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { type StudioModel } from "../../hooks/useMessageStudio";
import { Field } from "../../../../components/Field";

export function MessageFields({
  draft,
  set,
  kitKey,
}: Pick<StudioModel, "draft" | "set"> & { kitKey: StudioModel["key"] }) {
  const key = kitKey;
  return (
    <Panel title={t("MESSAGE")}>
      <div className="form-grid form-grid-2">
        <Field label={deferredText("Kit name")} htmlFor="kit-name">
          <input
            id="kit-name"
            className="pixel-input"
            maxLength={60}
            value={draft.name}
            onChange={(event) => set("name", event.target.value)}
          />
        </Field>
        <Field label={deferredText("Template key")} htmlFor="kit-key">
          <input id="kit-key" className="pixel-input" value={key || slug(draft.name)} readOnly />
        </Field>
      </div>
      <Field
        label={t("Content")}
        htmlFor="kit-content"
        hint={deferredText(
          "Use {user}, {server}, {level}, {ordinal}, and if/else for booster, returning, account_age_days>=7, or ordinal>=100.",
        )}
      >
        <textarea
          id="kit-content"
          className="pixel-input"
          rows={5}
          maxLength={2000}
          value={draft.content}
          onChange={(event) => set("content", event.target.value)}
        />
        <span className="counter">{draft.content.length}/2000</span>
        <span className="muted">
          {deferredText("Example: {{if returning}}Welcome back{{else}}Welcome{{end}}")}
        </span>
      </Field>
      <label className="studio-check">
        <input
          type="checkbox"
          checked={draft.allow_mass_mentions}
          onChange={(event) => set("allow_mass_mentions", event.target.checked)}
        />{" "}
        {deferredText("Admin allows @everyone and role pings from this kit")}
      </label>
      <p className="muted">
        {deferredText("Pings are blocked unless an admin explicitly enables them here.")}
      </p>
    </Panel>
  );
}
