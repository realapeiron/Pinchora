import { useOutletContext } from "react-router";

import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { ChannelPicker } from "../components/Pickers";
import { PixelToggle } from "../components/PixelToggle";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import type { Overview } from "../lib/api";
import { useDiscord } from "../lib/queries";
import { useSettingsForm } from "../lib/useSettingsForm";
import { PageHead } from "./PageHead";

type LogField = "log_members" | "log_names" | "log_bans" | "log_voice";

const KINDS: { field: LogField; title: string; example: string }[] = [
  { field: "log_members", title: "Joins and leaves", example: "📥 Mia joined · account 3 days old" },
  { field: "log_names", title: "Name changes", example: "✏️ Mia is now “Mia from Reef”" },
  { field: "log_bans", title: "Bans and unbans", example: "🔨 Spammer was banned" },
  { field: "log_voice", title: "Voice", example: "🔊 Mia moved from Lounge to Music" },
];

export function ServerLog() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const discord = useDiscord(guild);
  const form = useSettingsForm(guild, data.settings, () => []);
  const { value, set, errors } = form;
  const on = value.log_channel_id !== null;

  return (
    <div className="settings-page">
      <PageHead
        title="SERVER LOG"
        lead="A channel where the bot notes who came, who left, and what changed."
      />

      <Panel title="LOG CHANNEL">
        <Field
          label="Channel"
          htmlFor="log-channel"
          error={errors.log_channel_id}
          hint="Picking a channel turns the log on. Keep it private to moderators."
        >
          <ChannelPicker
            id="log-channel"
            info={discord.data}
            value={value.log_channel_id}
            onChange={(id) => set("log_channel_id", id)}
            none="Off: no server log"
            invalid={!!errors.log_channel_id}
          />
        </Field>
      </Panel>

      <Panel title="WHAT IT REPORTS">
        <div className={`stack ${on ? "" : "is-muted"}`}>
          {KINDS.map((kind) => (
            <SwitchRow
              key={kind.field}
              title={kind.title}
              control={
                <PixelToggle
                  label={kind.title}
                  checked={value[kind.field]}
                  onChange={(next) => set(kind.field, next)}
                />
              }
            >
              <span className="log-example">{kind.example}</span>
            </SwitchRow>
          ))}
        </div>
        {!on && <p className="notice">Pick a log channel above; until then nothing is reported.</p>}
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
