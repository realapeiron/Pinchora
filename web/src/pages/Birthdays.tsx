import { useState } from "react";
import { Link, useOutletContext } from "react-router";
import { ChannelPicker, RolePicker } from "../components/Pickers";
import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import { ApiError, type Overview } from "../lib/api";
import { type BirthdaySettings, useBirthdays, useSaveBirthdays } from "../lib/birthdays";
import { useDiscord } from "../lib/queries";
import { Loading, Problem } from "./States";
import { PageHead } from "./PageHead";
import "../styles/birthdays.css";

export function Birthdays() {
  const overview = useOutletContext<Overview>();
  const guild = overview.guild.id;
  const query = useBirthdays(guild);
  return (
    <div className="settings-page birthdays-page">
      <PageHead
        title="BIRTHDAYS"
        lead="A small celebration for members who choose to share their day. Their birth year stays private."
      />
      {query.isPending && <Loading label="Reading birthdays" />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && (
        <BirthdayBody
          key={JSON.stringify(query.data.settings)}
          guild={guild}
          saved={query.data.settings}
          upcoming={query.data.upcoming}
          enabled={overview.modules.find((m) => m.key === "birthdays")?.enabled ?? true}
        />
      )}
    </div>
  );
}
function BirthdayBody({
  guild,
  saved,
  upcoming,
  enabled,
}: {
  guild: string;
  saved: BirthdaySettings;
  upcoming: { user_id: string; name: string; date: string; month: number; day: number }[];
  enabled: boolean;
}) {
  const discord = useDiscord(guild);
  const save = useSaveBirthdays(guild);
  const [draft, setDraft] = useState(saved);
  const [preview, setPreview] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const change = <K extends keyof BirthdaySettings>(key: K, value: BirthdaySettings[K]) =>
    setDraft((old) => ({ ...old, [key]: value }));
  const count = (Object.keys(saved) as (keyof BirthdaySettings)[]).filter(
    (k) => saved[k] !== draft[k],
  ).length;
  const problem = save.error instanceof ApiError ? save.error : null;
  const fieldError = (key: string) => problem?.errors.find((e) => e.field === key)?.message;
  const invalid =
    (!draft.channel_id && draft.enabled) ||
    draft.message.length > 500 ||
    !draft.message.includes("{user}") ||
    draft.shells_gift < 0 ||
    draft.shells_gift > 500 ||
    !draft.server_tz.trim();
  const sample = draft.message.replaceAll("{user}", "@Mira").replaceAll("{age}", draft.allow_age ? "24" : "");
  return (
    <>
      {!enabled && (
        <p className="notice" role="note">
          The Birthdays module is off. Switch it on in <Link to={`/g/${guild}/modules`}>Modules</Link> to run
          celebrations.
        </p>
      )}
      <Panel title="CELEBRATIONS" aside={<span className="pixel-label muted">OPT IN</span>}>
        <p className="panel-lead muted">
          Members save their date with <code>/birthday set</code>. A birthday is hidden from others until they
          choose to share it. Only members Pinchora has seen in this server appear in the upcoming list.
        </p>
        <SwitchRow
          title="Announce birthdays"
          control={
            <PixelToggle
              label="Announce birthdays"
              checked={draft.enabled}
              onChange={(v) => change("enabled", v)}
            />
          }
        >
          Post a pixel cake card once on each member’s local birthday.
        </SwitchRow>
        <div className="birthday-fields">
          <Field
            label="Announcement channel"
            htmlFor="bd-channel"
            error={fieldError("channel_id")}
            hint="A text or announcement channel the bot can post in."
          >
            <ChannelPicker
              id="bd-channel"
              info={discord.data}
              value={draft.channel_id}
              onChange={(v) => change("channel_id", v)}
              none="Choose a channel"
            />
          </Field>
          <Field
            label="Birthday role"
            htmlFor="bd-role"
            error={fieldError("role_id")}
            hint="Given for 24 hours, then removed by the timed roles ticker."
          >
            <RolePicker
              id="bd-role"
              info={discord.data}
              value={draft.role_id}
              onChange={(v) => change("role_id", v)}
              assignable
              none="No birthday role"
            />
          </Field>
          <Field
            label="Server time zone"
            htmlFor="bd-zone"
            error={fieldError("server_tz")}
            hint="Default for members who do not pick their own IANA time zone."
          >
            <input
              id="bd-zone"
              className="pixel-input"
              value={draft.server_tz}
              onChange={(e) => change("server_tz", e.target.value)}
              list="birthday-zones"
            />
            <datalist id="birthday-zones">
              <option value="UTC" />
              <option value="Europe/Istanbul" />
              <option value="Europe/London" />
              <option value="America/New_York" />
              <option value="America/Los_Angeles" />
              <option value="Asia/Tokyo" />
            </datalist>
          </Field>
          <Field
            label="Shell gift"
            htmlFor="bd-gift"
            error={fieldError("shells_gift")}
            hint="0–500 shells. System paid once a year per player across servers."
          >
            <input
              id="bd-gift"
              type="number"
              min={0}
              max={500}
              className="pixel-input"
              value={draft.shells_gift}
              onChange={(e) => change("shells_gift", Number(e.target.value))}
            />
          </Field>
        </div>
      </Panel>
      <Panel
        title="THE MESSAGE"
        aside={<span className="pixel-label muted">{draft.message.length}/500 used</span>}
      >
        <Field
          label="Birthday message"
          htmlFor="bd-message"
          error={fieldError("message")}
          hint="Use {user} for the member and {age} only when age sharing is allowed."
        >
          <textarea
            id="bd-message"
            className="pixel-input birthday-message"
            value={draft.message}
            maxLength={500}
            onChange={(e) => change("message", e.target.value)}
          />
        </Field>
        <div className="birthday-chips">
          <button type="button" className="chip" onClick={() => change("message", draft.message + " {user}")}>
            + {"{user}"}
          </button>
          <button type="button" className="chip" onClick={() => change("message", draft.message + " {age}")}>
            + {"{age}"}
          </button>
        </div>
        <SwitchRow
          title="Allow ages"
          control={
            <PixelToggle
              label="Allow ages"
              checked={draft.allow_age}
              onChange={(v) => change("allow_age", v)}
            />
          }
        >
          Ages appear only for members who also choose to show theirs.
        </SwitchRow>
        <SwitchRow
          title="Cake Day badge"
          control={
            <PixelToggle label="Cake Day badge" checked={draft.badge} onChange={(v) => change("badge", v)} />
          }
        >
          Put a Cake Day ribbon on the birthday card.
        </SwitchRow>
        <PixelButton variant="secondary" onClick={() => setPreview((v) => !v)}>
          {preview ? "Hide preview" : "Preview card"}
        </PixelButton>
        {preview && (
          <div className="birthday-preview" role="img" aria-label="Birthday card preview">
            <div className="birthday-cake" aria-hidden="true">
              <i />
              <b />
              <strong />
            </div>
            <div>
              <span className="pixel-label">HAPPY BIRTHDAY</span>
              <h3>Mira</h3>
              <p>{sample}</p>
              {draft.badge && <span className="birthday-ribbon">✦ CAKE DAY</span>}
            </div>
          </div>
        )}
      </Panel>
      <Panel
        title="UPCOMING 30 DAYS"
        aside={<span className="pixel-label muted">{upcoming.length} shared</span>}
      >
        <p className="panel-lead muted">
          Only members who allow others to see their birthday appear here. Years and ages stay off this list.
        </p>
        {upcoming.length === 0 ? (
          <p className="empty-row muted">No shared birthdays in the next 30 days.</p>
        ) : (
          <ul className="birthday-upcoming">
            {upcoming.map((row) => (
              <li key={row.user_id}>
                <time dateTime={row.date}>
                  {new Date(`${row.date}T12:00:00`).toLocaleDateString(undefined, {
                    month: "short",
                    day: "numeric",
                  })}
                </time>
                <span>{row.name}</span>
                <span className="pixel-label">#{row.user_id.slice(-4)}</span>
              </li>
            ))}
          </ul>
        )}
      </Panel>
      <SaveBar
        count={count}
        saving={save.isPending}
        error={problem?.message}
        blocked={invalid}
        onReset={() => {
          setDraft(saved);
          save.reset();
        }}
        onSave={() => save.mutate(draft, { onSuccess: () => setToast("Birthday settings saved.") })}
      />
      <Toast message={toast} onDone={() => setToast(null)} />
    </>
  );
}
