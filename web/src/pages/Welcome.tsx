import { useOutletContext } from "react-router";

import { Field, SwitchRow } from "../components/Field";
import { MessageField } from "../components/MessageField";
import { Panel } from "../components/Panel";
import { ChannelPicker, RolePicker } from "../components/Pickers";
import { PixelToggle } from "../components/PixelToggle";
import { PreviewFrame } from "../components/PreviewFrame";
import { RichText } from "../components/RichText";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import type { FieldError, Overview, Settings } from "../lib/api";
import { useDiscord } from "../lib/queries";
import { ACCENTS, DEFAULTS, LIMITS, THEMES, checkAccent, checkMessage, checkTitle } from "../lib/rules";
import { useSettingsForm } from "../lib/useSettingsForm";
import { PageHead } from "./PageHead";

/** The card's size as the bot draws it. */
const CARD = { width: 1100, height: 500 };

function check(value: Settings): (FieldError | null)[] {
  const fail = <T,>(c: { ok: true; value: T } | { ok: false; field: string; message: string }) =>
    c.ok ? null : { field: c.field, message: c.message };
  return [
    value.welcome_message === null ? null : fail(checkMessage("welcome", value.welcome_message)),
    value.goodbye_message === null ? null : fail(checkMessage("goodbye", value.goodbye_message)),
    value.welcome_title === null ? null : fail(checkTitle(value.welcome_title)),
    value.welcome_accent === null ? null : fail(checkAccent(value.welcome_accent)),
  ];
}

export function Welcome() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const discord = useDiscord(guild);
  const form = useSettingsForm(guild, data.settings, check);
  const { value, set, errors } = form;
  const joinRole = discord.data?.roles.find((r) => r.id === value.join_role_id);
  const theme = value.welcome_theme ?? DEFAULTS.theme;
  const accentOk = value.welcome_accent === null || checkAccent(value.welcome_accent).ok;
  const titleOk = value.welcome_title === null || checkTitle(value.welcome_title).ok;
  const design =
    accentOk && titleOk
      ? {
          theme,
          title: value.welcome_title?.trim() || undefined,
          accent: value.welcome_accent ?? undefined,
        }
      : null;
  const themeAccent = THEMES.find((t) => t.key === theme)?.accent ?? "#ff6b4a";

  return (
    <div className="settings-page">
      <PageHead
        title="WELCOME"
        lead="Greet new members with a card, give them a role, and say goodbye when someone leaves."
      />

      <Panel title="WELCOME MESSAGES">
        <SwitchRow
          title="Welcome cards"
          control={
            <PixelToggle
              label="Welcome cards"
              checked={value.welcome_enabled}
              onChange={(on) => set("welcome_enabled", on)}
            />
          }
        >
          Post a card with the new member&apos;s avatar when someone joins. Needs the Server Members intent.
        </SwitchRow>
        <div className="form-grid">
          <Field
            label="Channel"
            htmlFor="welcome-channel"
            error={errors.welcome_channel_id}
            hint={
              value.welcome_enabled && !value.welcome_channel_id
                ? "Pick a channel, or no card is posted."
                : "Text or announcement channels the bot can see."
            }
          >
            <ChannelPicker
              id="welcome-channel"
              info={discord.data}
              value={value.welcome_channel_id}
              onChange={(id) => set("welcome_channel_id", id)}
              none="Not set"
              invalid={!!errors.welcome_channel_id}
            />
          </Field>
          <MessageField
            guild={guild}
            kind="welcome"
            label="Text above the card"
            value={value.welcome_message}
            onChange={(text) => set("welcome_message", text)}
            error={errors.welcome_message}
          />
        </div>
      </Panel>

      <Panel title="WELCOME CARD">
        <div className="designer">
          <div className="designer-controls">
            <Field label="Theme" htmlFor="theme-reef">
              <div className="swatches" role="radiogroup" aria-label="Card theme">
                {THEMES.map((t) => (
                  <button
                    key={t.key}
                    id={`theme-${t.key}`}
                    type="button"
                    role="radio"
                    aria-checked={theme === t.key}
                    className={`theme-swatch ${theme === t.key ? "is-checked" : ""}`}
                    onClick={() => set("welcome_theme", t.key)}
                  >
                    <span
                      className="theme-chip"
                      style={{ background: `linear-gradient(${t.top}, ${t.bottom})`, borderColor: t.edge }}
                    >
                      <span className="theme-dot" style={{ background: t.accent }} />
                    </span>
                    <span className="theme-name">{t.name.replace(/ \(.*\)$/, "")}</span>
                  </button>
                ))}
              </div>
            </Field>
            <Field
              label="Headline"
              htmlFor="welcome-title"
              error={errors.welcome_title}
              hint={`Big letters on the card, up to ${LIMITS.welcome_title_max} characters. Empty is “${DEFAULTS.welcome_title}”.`}
              aside={
                <span className="counter">
                  {[...(value.welcome_title ?? "")].length}/{LIMITS.welcome_title_max}
                </span>
              }
            >
              <input
                id="welcome-title"
                className="pixel-input"
                value={value.welcome_title ?? ""}
                placeholder={DEFAULTS.welcome_title.toUpperCase()}
                onChange={(event) =>
                  set("welcome_title", event.target.value === "" ? null : event.target.value)
                }
                aria-invalid={errors.welcome_title ? true : undefined}
              />
            </Field>
            <Field
              label="Accent colour"
              htmlFor="welcome-accent"
              error={errors.welcome_accent}
              hint="The frame and the name. Empty uses the theme's own colour."
            >
              <div className="accent-row">
                <span
                  className="accent-preview"
                  style={{ background: accentOk ? (value.welcome_accent ?? themeAccent) : "transparent" }}
                  aria-hidden="true"
                />
                <input
                  id="welcome-accent"
                  className="pixel-input accent-input"
                  value={value.welcome_accent ?? ""}
                  placeholder={themeAccent}
                  maxLength={7}
                  spellCheck={false}
                  onChange={(event) =>
                    set("welcome_accent", event.target.value === "" ? null : event.target.value)
                  }
                  aria-invalid={errors.welcome_accent ? true : undefined}
                />
              </div>
              <div className="palette" role="group" aria-label="Accent presets">
                <button
                  type="button"
                  className={`palette-swatch palette-theme ${value.welcome_accent === null ? "is-checked" : ""}`}
                  title="The theme's colour"
                  aria-label="The theme's colour"
                  style={{ background: themeAccent }}
                  onClick={() => set("welcome_accent", null)}
                />
                {ACCENTS.map((a) => (
                  <button
                    key={a.hex}
                    type="button"
                    className={`palette-swatch ${value.welcome_accent?.toLowerCase() === a.hex ? "is-checked" : ""}`}
                    title={a.name}
                    aria-label={a.name}
                    style={{ background: a.hex }}
                    onClick={() => set("welcome_accent", a.hex)}
                  />
                ))}
              </div>
            </Field>
          </div>
          <div className="designer-preview">
            <PreviewFrame
              path={`/api/guilds/${guild}/preview/welcome-card`}
              body={design}
              alt="The welcome card a new member gets"
              width={CARD.width}
              height={CARD.height}
            />
            <p className="muted fine">
              Drawn by the bot, with you as the new member. Saving keeps the design; this preview never posts
              anything.
            </p>
          </div>
        </div>
      </Panel>

      <Panel title="JOIN ROLE">
        <Field
          label="Role new members get"
          htmlFor="join-role"
          error={errors.join_role_id ?? (joinRole && !joinRole.assignable ? joinRole.why_not : null)}
          hint="Bots are skipped. The bot's role must sit above it."
        >
          <RolePicker
            id="join-role"
            info={discord.data}
            value={value.join_role_id}
            onChange={(id) => set("join_role_id", id)}
            none="No role"
            assignable
            invalid={!!errors.join_role_id}
          />
        </Field>
        {discord.data && !discord.data.bot.manage_roles && (
          <p className="notice notice-heart">
            <RichText text="The bot lacks **Manage Roles**, so it can't hand out any role yet." />
          </p>
        )}
      </Panel>

      <Panel title="GOODBYE">
        <SwitchRow
          title="Goodbye messages"
          control={
            <span className={`badge ${value.goodbye_channel_id ? "badge-mint" : "badge-dim"}`}>
              {value.goodbye_channel_id ? "ON" : "OFF"}
            </span>
          }
        >
          Say goodbye in a channel when a member leaves. Pick a channel to turn it on.
        </SwitchRow>
        <div className="form-grid">
          <Field label="Channel" htmlFor="goodbye-channel" error={errors.goodbye_channel_id}>
            <ChannelPicker
              id="goodbye-channel"
              info={discord.data}
              value={value.goodbye_channel_id}
              onChange={(id) => set("goodbye_channel_id", id)}
              none="Off: no goodbye messages"
              invalid={!!errors.goodbye_channel_id}
            />
          </Field>
          <MessageField
            guild={guild}
            kind="goodbye"
            label="Goodbye text"
            value={value.goodbye_message}
            onChange={(text) => set("goodbye_message", text)}
            error={errors.goodbye_message}
          />
        </div>
      </Panel>

      <SaveBar
        count={form.changes}
        saving={form.saving}
        blocked={form.blocked}
        error={form.barError}
        onSave={form.submit}
        onReset={form.reset}
      />
      <Toast message={form.toast} onDone={form.clearToast} />
    </div>
  );
}
