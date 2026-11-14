import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../../i18n";
import { Field } from "../../../../components/Field";
import { Panel } from "../../../../components/Panel";
import { RolePicker } from "../../../../components/DashboardPickers";
import type { GuildInfo, ResponderDef, TriggerKind } from "../../../../lib/api";
import { LIMITS, MATCH_KINDS, TRIGGERS } from "../../../../lib/tags";
import type { TagEditor } from "../../hooks/useTagEditor";

type Props = {
  draft: Pick<ResponderDef, "trigger_kind" | "trigger_value" | "match_kind">;
  set: TagEditor<ResponderDef>["set"];
  problem: TagEditor<ResponderDef>["problem"];
  info: GuildInfo | undefined;
  keywordLocked: boolean;
  pickTrigger: (next: TriggerKind) => void;
};

export function ResponderTrigger({ draft, set, problem, info, keywordLocked, pickTrigger }: Props) {
  const kind = draft.trigger_kind;
  return (
    <Panel title={deferredText("WHEN IT FIRES")}>
      <div className="choice choice-triggers" role="radiogroup" aria-label={spaTranslate("Trigger")}>
        {TRIGGERS.map((t) => {
          const locked = t.needs_message_content && keywordLocked && kind !== t.key;
          return (
            <label
              key={t.key}
              className={`choice-card ${kind === t.key ? "is-checked" : ""} ${locked ? "is-locked" : ""}`}
            >
              <input
                type="radio"
                name="Trigger"
                value={t.key}
                checked={kind === t.key}
                disabled={locked}
                onChange={() => pickTrigger(t.key)}
              />
              <span className="choice-title">
                {spaTranslate(t.label)}
                {t.needs_message_content && keywordLocked && (
                  <span className="badge badge-gold choice-lock">{spaTranslate("LOCKED")}</span>
                )}
              </span>
              {t.needs_message_content && keywordLocked && (
                <span className="choice-detail muted" data-testid="keyword-reason">
                  {" "}
                  {deferredText("Needs the Message Content intent, which this bot runs without.")}{" "}
                </span>
              )}
            </label>
          );
        })}
      </div>
      {(kind === "role_added" || kind === "role_removed") && (
        <Field label={deferredText("Which role")} htmlFor="responder-role" error={problem("trigger_value")}>
          <RolePicker
            id="responder-role"
            info={info}
            value={draft.trigger_value}
            onChange={(next) => set("trigger_value", next)}
            none={spaTranslate("Pick a role")}
          />
        </Field>
      )}
      {kind === "level_up" && (
        <Field
          label={deferredText("From level")}
          htmlFor="responder-level"
          error={problem("trigger_value")}
          hint={deferredText("Fires for every level-up from this level on. Empty: every level-up.")}
        >
          <input
            id="responder-level"
            type="number"
            className="pixel-input input-short"
            min={1}
            max={1000}
            value={draft.trigger_value ?? ""}
            onChange={(event) => set("trigger_value", event.target.value === "" ? null : event.target.value)}
          />
        </Field>
      )}
      {kind === "keyword" && (
        <>
          <div className="form-grid form-grid-2">
            <Field
              label={deferredText("Word or phrase")}
              htmlFor="responder-keyword"
              error={problem("trigger_value")}
              hint={deferredText("Capital letters don't matter.")}
            >
              <input
                id="responder-keyword"
                className="pixel-input"
                value={draft.trigger_value ?? ""}
                maxLength={LIMITS.keyword_max}
                onChange={(event) => set("trigger_value", event.target.value)}
              />
            </Field>
            <Field
              label={deferredText("How it matches")}
              htmlFor="responder-match"
              error={problem("match_kind")}
            >
              <select
                id="responder-match"
                className="pixel-input pixel-select"
                value={draft.match_kind ?? "contains"}
                onChange={(event) => set("match_kind", event.target.value as ResponderDef["match_kind"])}
              >
                {MATCH_KINDS.map((m) => (
                  <option key={m.value} value={m.value}>
                    {spaTranslate(m.title)}: {spaTranslate(m.detail)}
                  </option>
                ))}
              </select>
            </Field>
          </div>
          <p className="field-hint muted">
            {" "}
            {deferredText(
              "One answer per message at most: if several responders match, the first that is ready answers.",
            )}{" "}
          </p>
        </>
      )}
    </Panel>
  );
}
