import { useId, useState } from "react";
import { translate as spaTranslate } from "../../../i18n";
import { Choice } from "../../../components/Choice";
import { Field } from "../../../components/Field";
import { SettingsRow, FieldGrid } from "../../../components/SettingsRow";
import { t } from "../../../i18n/dashboard";
import { ticketSettingsSummary } from "./ticketsSummary";
import { ChannelPicker } from "../../../components/Pickers";
import { SaveBar } from "../../../components/SaveBar";
import { ApiError, type GuildInfo } from "../../../lib/api";
import { type Limits, type TicketSettings, exampleName, useSaveTicketSettings } from "../api/tickets";
import { CategoryPicker } from "./TicketPanelFields";

const MODES = [
  {
    value: "auto" as const,
    title: "Automatic",
    detail: "Channels, then private threads once the server has 400 channels.",
  },
  {
    value: "channel" as const,
    title: "Channels",
    detail: "Each ticket is a channel in the ticket category.",
  },
  {
    value: "thread" as const,
    title: "Private threads",
    detail: "Each ticket is a private thread off the panel's channel.",
  },
];

export function SettingsForm({
  guild,
  saved,
  limits,
  info,
  onToast,
}: {
  guild: string;
  saved: TicketSettings;
  limits: Limits;
  info: GuildInfo | undefined;
  onToast: (message: string) => void;
}) {
  const [value, setValue] = useState<TicketSettings>(saved);
  const save = useSaveTicketSettings(guild);
  const base = useId();
  const set = <K extends keyof TicketSettings>(field: K, next: TicketSettings[K]) =>
    setValue((v) => ({ ...v, [field]: next }));
  const changes = (Object.keys(saved) as (keyof TicketSettings)[]).filter(
    (k) => saved[k] !== value[k],
  ).length;
  const refused = save.error instanceof ApiError ? save.error.errors : [];
  const errors: Record<string, string> = {};
  for (const e of refused) errors[e.field] ??= e.message;
  const local: string[] = [];
  if (
    !Number.isInteger(value.auto_close_hours) ||
    value.auto_close_hours < 0 ||
    value.auto_close_hours > limits.auto_close_hours
  ) {
    errors.auto_close_hours ??= spaTranslate("0 (never) to {arg1} hours.", { arg1: limits.auto_close_hours });
    local.push("auto_close_hours");
  }
  if (
    !Number.isInteger(value.max_open_per_user) ||
    value.max_open_per_user < 1 ||
    value.max_open_per_user > limits.open_per_user
  ) {
    errors.max_open_per_user ??= `1 to ${limits.open_per_user} tickets.`;
    local.push("max_open_per_user");
  }
  const example = exampleName(value.naming);
  if (!example) {
    errors.naming ??= "The name needs letters or numbers, like `ticket-{number}`.";
    local.push("naming");
  }

  const summary = ticketSettingsSummary(value);
  return (
    <>
      <div className="settings-detail-list">
        <SettingsRow
          title={t("Ticket categories and names")}
          description={t("Choose categories, ticket location and channel names.")}
          summary={summary.placement}
        >
          <FieldGrid>
            <Field
              label={spaTranslate("Ticket category")}
              htmlFor={`${base}-open`}
              error={errors.open_category_id}
              hint={spaTranslate("Where ticket channels are made. A panel can pick its own.")}
            >
              <CategoryPicker
                id={`${base}-open`}
                info={info}
                value={value.open_category_id}
                onChange={(id) => set("open_category_id", id)}
                none={spaTranslate("No category (top of the list)")}
                invalid={!!errors.open_category_id}
              />
            </Field>
            <Field
              label={spaTranslate("Closed category")}
              htmlFor={`${base}-closed`}
              error={errors.closed_category_id}
              hint={spaTranslate("Closed tickets move here. None: they stay where they are.")}
            >
              <CategoryPicker
                id={`${base}-closed`}
                info={info}
                value={value.closed_category_id}
                onChange={(id) => set("closed_category_id", id)}
                none={spaTranslate("Don't move closed tickets")}
                invalid={!!errors.closed_category_id}
              />
            </Field>
            <Field
              label={spaTranslate("Channel name")}
              htmlFor={`${base}-naming`}
              error={errors.naming}
              hint={
                <span className="tk-sample">
                  {"{number}"}, {"{user}"} {spaTranslate("and")} {"{topic}"} {spaTranslate("are filled in:")}{" "}
                  <code>#{example || "…"}</code>
                </span>
              }
            >
              <input
                id={`${base}-naming`}
                className="pixel-input"
                value={value.naming}
                maxLength={limits.naming}
                spellCheck={false}
                onChange={(e) => set("naming", e.target.value)}
                aria-invalid={errors.naming ? true : undefined}
              />
            </Field>
          </FieldGrid>
          <Field label={spaTranslate("Tickets are")} htmlFor={`${base}-mode`}>
            <Choice
              name="Where tickets live"
              value={value.mode}
              onChange={(mode) => set("mode", mode)}
              options={MODES}
            />
          </Field>
        </SettingsRow>
        <SettingsRow
          title={t("Transcripts")}
          description={t("Choose where ticket records go.")}
          summary={summary.transcripts}
          enabled={value.dm_transcript}
          onToggle={(on) => set("dm_transcript", on)}
          toggleLabel={spaTranslate("Transcript by direct message")}
        >
          <Field
            label={spaTranslate("Log channel")}
            htmlFor={`${base}-log`}
            error={errors.log_channel_id}
            hint={spaTranslate("Opened, claimed and closed tickets, with each transcript.")}
          >
            <ChannelPicker
              id={`${base}-log`}
              info={info}
              value={value.log_channel_id}
              onChange={(id) => set("log_channel_id", id)}
              none={spaTranslate("No log")}
              invalid={!!errors.log_channel_id}
            />
          </Field>
          <p className="panel-lead muted">
            {spaTranslate(
              "The member gets their ticket's transcript when it closes (unless they turned server DMs off).",
            )}
          </p>
        </SettingsRow>
        <SettingsRow
          title={t("Ticket limits and auto-close")}
          description={t("Limit open tickets and close idle ones.")}
          summary={summary.limits}
        >
          <FieldGrid>
            <Field
              label={spaTranslate("Auto-close after (hours)")}
              htmlFor={`${base}-auto`}
              error={errors.auto_close_hours}
              hint={
                value.auto_close_hours === 0
                  ? spaTranslate("0: tickets never close by themselves.")
                  : spaTranslate("A ticket nobody writes in for {arg1} h closes.", {
                      arg1: value.auto_close_hours,
                    })
              }
            >
              <input
                id={`${base}-auto`}
                type="number"
                min={0}
                max={limits.auto_close_hours}
                className="pixel-input input-short"
                value={Number.isNaN(value.auto_close_hours) ? "" : value.auto_close_hours}
                onChange={(e) => set("auto_close_hours", e.target.valueAsNumber)}
                aria-invalid={errors.auto_close_hours ? true : undefined}
              />
            </Field>
            <Field
              label={spaTranslate("Open tickets per member")}
              htmlFor={`${base}-max`}
              error={errors.max_open_per_user}
              hint={`1 to ${limits.open_per_user}. One ticket a minute per member either way.`}
            >
              <input
                id={`${base}-max`}
                type="number"
                min={1}
                max={limits.open_per_user}
                className="pixel-input input-short"
                value={Number.isNaN(value.max_open_per_user) ? "" : value.max_open_per_user}
                onChange={(e) => set("max_open_per_user", e.target.valueAsNumber)}
                aria-invalid={errors.max_open_per_user ? true : undefined}
              />
            </Field>
          </FieldGrid>
        </SettingsRow>
        <SettingsRow
          title={spaTranslate("Ratings")}
          description={t("Ask members to rate closed tickets.")}
          summary={summary.ratings}
          enabled={value.rating_enabled}
          onToggle={(on) => set("rating_enabled", on)}
          toggleLabel={spaTranslate("Ratings")}
        >
          <p className="panel-lead muted">
            {spaTranslate(
              "Ask the member for one to five stars after a close; the numbers above show the average.",
            )}
          </p>
        </SettingsRow>
      </div>
      <SaveBar
        count={changes}
        saving={save.isPending}
        blocked={local.length > 0}
        error={
          local.length > 0
            ? spaTranslate("Fix the marked fields first")
            : save.isError && refused.length === 0
              ? save.error.message
              : null
        }
        onSave={() =>
          save.mutate(value, {
            onSuccess: () => onToast(spaTranslate("Saved. New tickets use it right away.")),
          })
        }
        onReset={() => {
          setValue(saved);
          save.reset();
        }}
      />
    </>
  );
}
