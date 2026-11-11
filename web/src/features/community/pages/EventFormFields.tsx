import type { Dispatch, SetStateAction } from "react";
import { Field } from "../../../components/Field";
import { ChannelPicker, RolePicker } from "../../../components/Pickers";
import type { GuildInfo } from "../../../lib/api";
import { catalogWords } from "../../admin";
import { zonedInput } from "../api/events";
import type { EventDraft } from "./EventEditor";

export function EventFormFields({
  editing,
  setEditing,
  info,
  t,
  changeTime,
  setNotice,
}: {
  editing: EventDraft;
  setEditing: Dispatch<SetStateAction<EventDraft | null>>;
  info?: GuildInfo;
  t: ReturnType<typeof catalogWords>;
  changeTime: (field: "starts_at" | "ends_at", value: string) => void;
  setNotice: (value: string) => void;
}) {
  return (
    <>
      <Field label={t("Title")} htmlFor="ev-title">
        <input
          id="ev-title"
          className="pixel-input"
          maxLength={100}
          value={editing.body.title}
          onChange={(e) => setEditing({ ...editing, body: { ...editing.body, title: e.target.value } })}
        />
      </Field>
      <Field label={t("Description")} htmlFor="ev-description">
        <textarea
          id="ev-description"
          className="pixel-input"
          maxLength={1000}
          rows={3}
          value={editing.body.description}
          onChange={(e) => setEditing({ ...editing, body: { ...editing.body, description: e.target.value } })}
        />
      </Field>
      <div className="ev-form-pair">
        <Field label={t("Starts")} htmlFor="ev-start">
          <input
            id="ev-start"
            type="datetime-local"
            className="pixel-input"
            value={zonedInput(editing.body.starts_at, editing.body.tz)}
            onChange={(e) => changeTime("starts_at", e.target.value)}
          />
        </Field>
        <Field label={t("Ends")} htmlFor="ev-end">
          <input
            id="ev-end"
            type="datetime-local"
            className="pixel-input"
            value={zonedInput(editing.body.ends_at, editing.body.tz)}
            onChange={(e) => changeTime("ends_at", e.target.value)}
          />
        </Field>
      </div>
      <p className="muted ev-time-note">
        {t("Enter the wall-clock time in {zone}. Members see it in their own time zone.", {
          zone: editing.body.tz,
        })}
      </p>
      <Field label={t("Event time zone")} htmlFor="ev-tz" hint={t("IANA name, such as Europe/Istanbul.")}>
        <input
          id="ev-tz"
          className="pixel-input"
          defaultValue={editing.body.tz}
          onBlur={(e) => {
            try {
              new Intl.DateTimeFormat("en", { timeZone: e.target.value });
              setEditing({ ...editing, body: { ...editing.body, tz: e.target.value } });
            } catch {
              setNotice(t("Use an IANA time zone such as Europe/Istanbul."));
            }
          }}
        />
      </Field>
      <Field label={t("Post RSVP card in")} htmlFor="ev-channel">
        <ChannelPicker
          id="ev-channel"
          info={info}
          value={editing.body.channel_id}
          onChange={(channel_id) => setEditing({ ...editing, body: { ...editing.body, channel_id } })}
          none={t("Choose a channel")}
        />
      </Field>
      <div className="ev-form-pair">
        <Field
          label={t("Going capacity")}
          htmlFor="ev-capacity"
          hint={t("0 means no limit; extra RSVPs wait in order.")}
        >
          <input
            id="ev-capacity"
            type="number"
            min={0}
            max={100000}
            className="pixel-input"
            value={editing.body.options[0]?.capacity ?? 0}
            onChange={(e) =>
              setEditing({
                ...editing,
                body: {
                  ...editing.body,
                  options: editing.body.options.map((o, i) =>
                    i === 0 ? { ...o, capacity: Number(e.target.value) } : o,
                  ),
                },
              })
            }
          />
        </Field>
        <Field label={t("Required role")} htmlFor="ev-role" hint={t("Leave empty for everyone.")}>
          <RolePicker
            id="ev-role"
            info={info}
            value={editing.body.required_role_id}
            none={t("Everyone can RSVP")}
            onChange={(required_role_id) =>
              setEditing({
                ...editing,
                body: { ...editing.body, required_role_id },
              })
            }
          />
        </Field>
      </div>
      {[1, 2, 3, ...(editing.body.options.length > 3 ? [4] : [])].map((index) => {
        const option = editing.body.options[index];
        const fixed = index < 3;
        return (
          <div className="ev-form-pair" key={index}>
            <Field
              label={
                fixed
                  ? t("{name} choice", { name: t(option?.label ?? t("Choice")) })
                  : t("Custom choice {number}", { number: index - 2 })
              }
              htmlFor={`ev-option-${index}`}
              hint={fixed ? t("Default RSVP choice") : t("Leave blank to omit this choice.")}
            >
              <input
                id={`ev-option-${index}`}
                className="pixel-input"
                maxLength={40}
                disabled={fixed}
                value={fixed ? t(option?.label ?? "") : (option?.label ?? "")}
                onChange={(e) => {
                  const options = [...editing.body.options];
                  const label = e.target.value;
                  if (label)
                    options[index] = {
                      key: `extra${index - 2}`,
                      label,
                      capacity: option?.capacity ?? 0,
                    };
                  else options.splice(index);
                  setEditing({ ...editing, body: { ...editing.body, options } });
                }}
              />
            </Field>
            {option && (
              <Field label={t("Capacity")} htmlFor={`ev-option-cap-${index}`} hint={t("0 means unlimited.")}>
                <input
                  id={`ev-option-cap-${index}`}
                  type="number"
                  min={0}
                  max={100000}
                  className="pixel-input"
                  value={option.capacity}
                  onChange={(e) =>
                    setEditing({
                      ...editing,
                      body: {
                        ...editing.body,
                        options: editing.body.options.map((o, i) =>
                          i === index ? { ...o, capacity: Number(e.target.value) } : o,
                        ),
                      },
                    })
                  }
                />
              </Field>
            )}
          </div>
        );
      })}
      <Field label={t("Location")} htmlFor="ev-location">
        <input
          id="ev-location"
          className="pixel-input"
          value={editing.body.location}
          onChange={(e) => setEditing({ ...editing, body: { ...editing.body, location: e.target.value } })}
        />
      </Field>
      <div className="ev-form-pair">
        <Field label={t("Repeat")} htmlFor="ev-repeat">
          <select
            id="ev-repeat"
            className="pixel-input pixel-select"
            value={editing.body.recurrence?.repeat ?? "off"}
            onChange={(e) =>
              setEditing({
                ...editing,
                body: {
                  ...editing.body,
                  recurrence:
                    e.target.value === "off"
                      ? null
                      : {
                          repeat: e.target.value as "daily" | "weekly",
                          until: editing.body.recurrence?.until ?? editing.body.starts_at.slice(0, 10),
                        },
                },
              })
            }
          >
            <option value="off">{t("No repeat")}</option>
            <option value="daily">{t("Daily")}</option>
            <option value="weekly">{t("Weekly")}</option>
          </select>
        </Field>
        {editing.body.recurrence && (
          <Field label={t("Until")} htmlFor="ev-until">
            <input
              id="ev-until"
              type="date"
              className="pixel-input"
              value={editing.body.recurrence.until}
              onChange={(e) =>
                setEditing({
                  ...editing,
                  body: {
                    ...editing.body,
                    recurrence: editing.body.recurrence && {
                      ...editing.body.recurrence,
                      until: e.target.value,
                    },
                  },
                })
              }
            />
          </Field>
        )}
      </div>
      <label className="ev-check">
        <input
          type="checkbox"
          checked={editing.body.sync}
          onChange={(e) => setEditing({ ...editing, body: { ...editing.body, sync: e.target.checked } })}
        />{" "}
        {t("Also create a Discord Scheduled Event")}
        <small>{t("Requires Manage Events for the bot.")}</small>
      </label>
    </>
  );
}
