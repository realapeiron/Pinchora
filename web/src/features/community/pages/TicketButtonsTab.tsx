import { translate as spaTranslate } from "../../../i18n";
import { Choice } from "../../../components/Choice";
import { Field } from "../../../components/Field";
import { ChannelPicker } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import type { Tone } from "../api/tickets";
import type { TabProps } from "./TicketPanelTabProps";

const TONES: { value: Tone; label: string }[] = [
  { value: "primary", label: "Blurple" },
  { value: "secondary", label: "Grey" },
  { value: "success", label: "Green" },
  { value: "danger", label: "Red" },
];

export function ButtonsTab({ draft, set, limits, info, errors, base, most, updateTopic }: TabProps) {
  return (
    <>
      <div className="form-grid form-grid-2">
        <Field
          label={spaTranslate("Name")}
          htmlFor={`${base}-name`}
          error={errors.name}
          hint={spaTranslate("How /community tickets and this page find it. Members never see it.")}
        >
          <input
            id={`${base}-name`}
            className="pixel-input"
            value={draft.name}
            maxLength={limits.name}
            onChange={(e) => set("name", e.target.value)}
            aria-invalid={errors.name ? true : undefined}
          />
        </Field>
        <Field
          label={spaTranslate("Posted in")}
          htmlFor={`${base}-channel`}
          error={errors.channel_id}
          hint={spaTranslate("Private threads open off this channel in thread mode.")}
        >
          <ChannelPicker
            id={`${base}-channel`}
            info={info}
            value={draft.channel_id}
            onChange={(id) => set("channel_id", id)}
            invalid={!!errors.channel_id}
          />
        </Field>
      </div>
      <Field
        label={spaTranslate("Title")}
        htmlFor={`${base}-title`}
        error={errors.title}
        aside={
          <span className="counter">
            {[...draft.title].length}/{limits.title}
          </span>
        }
      >
        <input
          id={`${base}-title`}
          className="pixel-input"
          value={draft.title}
          onChange={(e) => set("title", e.target.value)}
          aria-invalid={errors.title ? true : undefined}
        />
      </Field>
      <Field
        label={spaTranslate("Text")}
        htmlFor={`${base}-description`}
        error={errors.description}
        hint={spaTranslate("Markdown works: **bold**, *italic*. Mentions are dropped.")}
        aside={
          <span className="counter">
            {[...draft.description].length}/{limits.description}
          </span>
        }
      >
        <textarea
          id={`${base}-description`}
          className="pixel-input pixel-textarea"
          value={draft.description}
          onChange={(e) => set("description", e.target.value)}
          aria-invalid={errors.description ? true : undefined}
        />
      </Field>
      <Field
        label={spaTranslate("Colour")}
        htmlFor={`${base}-color`}
        error={errors.color}
        hint={spaTranslate("The stripe down the side.")}
      >
        <div className="tk-color">
          <input
            type="color"
            aria-label={spaTranslate("Pick the colour")}
            value={/^#[0-9a-fA-F]{6}$/.test(draft.color) ? draft.color : "#ff6b4a"}
            onChange={(e) => set("color", e.target.value)}
          />
          <input
            id={`${base}-color`}
            className="pixel-input input-short"
            value={draft.color}
            maxLength={7}
            spellCheck={false}
            onChange={(e) => set("color", e.target.value)}
            aria-invalid={errors.color ? true : undefined}
          />
        </div>
      </Field>
      <Field label={spaTranslate("Style")} htmlFor={`${base}-style-buttons`}>
        <Choice
          name="Panel style"
          value={draft.style}
          onChange={(style) => set("style", style)}
          options={[
            {
              value: "buttons",
              title: spaTranslate("Buttons"),
              detail: spaTranslate("One button per topic, up to {arg1}.", { arg1: limits.buttons }),
            },
            {
              value: "select",
              title: spaTranslate("Menu"),
              detail: spaTranslate("A drop-down of up to {arg1} topics.", { arg1: limits.options }),
            },
          ]}
        />
      </Field>
      <Field
        label={spaTranslate("Topics")}
        error={errors.buttons}
        aside={
          <span className={`counter ${draft.topics.length > most ? "is-over" : ""}`}>
            {draft.topics.length}/{most}
          </span>
        }
      >
        <ul className="tk-topics">
          {draft.topics.map((topic, n) => (
            <li key={n} className={`tk-topic ${draft.style === "select" ? "is-menu" : ""}`}>
              <input
                className="pixel-input tk-emoji"
                aria-label={spaTranslate("Topic {arg1} emoji", { arg1: n + 1 })}
                placeholder="🎫"
                value={topic.emoji ?? ""}
                maxLength={8}
                onChange={(e) => updateTopic(n, { emoji: e.target.value })}
              />
              <input
                className="pixel-input"
                aria-label={spaTranslate("Topic {arg1} label", { arg1: n + 1 })}
                value={topic.label}
                maxLength={limits.label}
                onChange={(e) => updateTopic(n, { label: e.target.value })}
              />
              {draft.style === "buttons" ? (
                <select
                  className="pixel-input pixel-select"
                  aria-label={spaTranslate("Topic {arg1} colour", { arg1: n + 1 })}
                  value={topic.tone}
                  onChange={(e) => updateTopic(n, { tone: e.target.value as Tone })}
                >
                  {TONES.map((t) => (
                    <option key={t.value} value={t.value}>
                      {t.label}
                    </option>
                  ))}
                </select>
              ) : (
                <input
                  className="pixel-input"
                  aria-label={spaTranslate("Topic {arg1} description", { arg1: n + 1 })}
                  placeholder={spaTranslate("A line under it (optional)")}
                  value={topic.description ?? ""}
                  maxLength={limits.option_description}
                  onChange={(e) => updateTopic(n, { description: e.target.value })}
                />
              )}
              <button
                type="button"
                className="tk-icon-button"
                aria-label={spaTranslate("Remove topic {arg1}", { arg1: n + 1 })}
                onClick={() =>
                  set(
                    "topics",
                    draft.topics.filter((_, i) => i !== n),
                  )
                }
              >
                ×
              </button>
            </li>
          ))}
        </ul>
        <PixelButton
          variant="secondary"
          className="button-small tk-add"
          disabled={draft.topics.length >= most}
          onClick={() =>
            set("topics", [...draft.topics, { label: "", emoji: null, description: null, tone: "primary" }])
          }
        >
          {" "}
          {spaTranslate("Add topic")}{" "}
        </PixelButton>
      </Field>
    </>
  );
}
