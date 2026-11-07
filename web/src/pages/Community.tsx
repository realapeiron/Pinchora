import { dateTime } from "../i18n";
import { translate as spaTranslate } from "../i18n";
import { useCallback, useState } from "react";
import { Link, useOutletContext } from "react-router";

import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { ChannelPicker } from "../components/Pickers";
import { PixelToggle } from "../components/PixelToggle";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import type { Community as CommunityData, ModuleEntry, Overview } from "../lib/api";
import { birthdayWords, useBirthdays } from "../lib/birthdays";
import { CommunitySaveError, useCommunity, useDiscord } from "../lib/queries";
import { Loading, Problem } from "./States";
import { PageHead } from "./PageHead";

const EVERY: Record<number, string> = {
  6: "Every 6 hours",
  12: "Every 12 hours",
  24: "Every day",
  48: "Every 2 days",
  168: "Every week",
};

export function Community() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const { query } = useCommunity(guild);
  const birthdays = useBirthdays(guild);
  const t = birthdayWords(birthdays.data?.strings, birthdays.data?.language);
  return (
    <div className="settings-page">
      <PageHead
        title={spaTranslate("COMMUNITY")}
        lead={t(
          "Where /suggest posts ideas, whether critters wash up while people chat, timed storm surges, and birthday celebrations.",
        )}
      />
      {query.isPending && <Loading label={spaTranslate("Reading the settings")} />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && <CommunityForm guild={guild} saved={query.data} modules={data.modules} />}
    </div>
  );
}

/** Says so when the module a section belongs to is switched off: its switch is the one that counts. */
function ModuleOff({ guild, modules, module }: { guild: string; modules: ModuleEntry[]; module: string }) {
  const found = modules.find((m) => m.key === module);
  if (!found || found.enabled) return null;
  return (
    <p className="notice" role="note">
      <strong>{spaTranslate(found.label)}</strong>
      {spaTranslate("is switched off in")} <Link to={`/g/${guild}/modules`}>{spaTranslate("Modules")}</Link>
      {spaTranslate(", so nothing here does anything until you switch it on.")}{" "}
    </p>
  );
}

function CommunityForm({
  guild,
  saved,
  modules,
}: {
  guild: string;
  saved: CommunityData;
  modules: ModuleEntry[];
}) {
  const discord = useDiscord(guild);
  const { save } = useCommunity(guild);
  const [suggestions, setSuggestions] = useState(saved.suggestions.channel_id);
  const [enabled, setEnabled] = useState(saved.critters.enabled);
  const [critterChannel, setCritterChannel] = useState(saved.critters.channel_id);
  const [surgeOn, setSurgeOn] = useState(saved.surge.enabled);
  const [surgeChannel, setSurgeChannel] = useState(saved.surge.channel_id);
  const [surgeEvery, setSurgeEvery] = useState(saved.surge.every_hours);
  const [toast, setToast] = useState<string | null>(null);
  const clearToast = useCallback(() => setToast(null), []);
  const suggestionsChanged = suggestions !== saved.suggestions.channel_id;
  const crittersChanged = enabled !== saved.critters.enabled || critterChannel !== saved.critters.channel_id;
  const surgeChanged =
    surgeOn !== saved.surge.enabled ||
    surgeChannel !== saved.surge.channel_id ||
    surgeEvery !== saved.surge.every_hours;
  const changes =
    Number(suggestionsChanged) +
    Number(enabled !== saved.critters.enabled) +
    Number(critterChannel !== saved.critters.channel_id) +
    Number(surgeOn !== saved.surge.enabled) +
    Number(surgeChannel !== saved.surge.channel_id) +
    Number(surgeEvery !== saved.surge.every_hours);
  // What the server refused, by the part of the page it was about.
  const refused = save.error instanceof CommunitySaveError ? save.error : null;
  const fieldError = (section: CommunitySaveError["section"], field: string) =>
    refused?.section === section && refused.reason.field === field ? refused.message : null;

  const submit = () =>
    save.mutate(
      {
        ...(suggestionsChanged ? { suggestions: { channel_id: suggestions } } : {}),
        ...(crittersChanged ? { critters: { enabled, channel_id: critterChannel } } : {}),
        ...(surgeChanged
          ? {
              surge: {
                ...saved.surge,
                enabled: surgeOn,
                channel_id: surgeChannel,
                every_hours: surgeEvery,
              },
            }
          : {}),
      },
      { onSuccess: () => setToast(spaTranslate("Saved. The bot uses it from the next message.")) },
    );
  const reset = () => {
    setSuggestions(saved.suggestions.channel_id);
    setEnabled(saved.critters.enabled);
    setCritterChannel(saved.critters.channel_id);
    setSurgeOn(saved.surge.enabled);
    setSurgeChannel(saved.surge.channel_id);
    setSurgeEvery(saved.surge.every_hours);
    save.reset();
  };

  return (
    <>
      <Panel title={spaTranslate("SUGGESTIONS")}>
        <ModuleOff guild={guild} modules={modules} module="suggestions" />
        <Field
          label={spaTranslate("Suggestion channel")}
          htmlFor="suggestion-channel"
          error={suggestionsChanged ? fieldError("suggestions", "channel_id") : null}
          hint={spaTranslate(
            "Members post ideas with /suggest; everyone votes with buttons, managers approve or deny.",
          )}
        >
          <ChannelPicker
            id="suggestion-channel"
            info={discord.data}
            value={suggestions}
            onChange={setSuggestions}
            none={spaTranslate("No channel yet: /suggest asks for one")}
          />
        </Field>
      </Panel>

      <Panel title={spaTranslate("CRITTER DROPS")}>
        <ModuleOff guild={guild} modules={modules} module="collection" />
        <SwitchRow
          title={spaTranslate("Chat drops")}
          control={<PixelToggle label={spaTranslate("Chat drops")} checked={enabled} onChange={setEnabled} />}
        >
          {" "}
          {spaTranslate(
            "While people chat, critters and message bottles now and then wash up for the quickest to grab. Off until a server turns it on.",
          )}{" "}
        </SwitchRow>
        <Field
          label={spaTranslate("Where they land")}
          htmlFor="critter-channel"
          error={crittersChanged ? fieldError("critters", "channel_id") : null}
          hint={spaTranslate("Only chat in a set channel counts towards a drop.")}
        >
          <ChannelPicker
            id="critter-channel"
            info={discord.data}
            value={critterChannel}
            onChange={setCritterChannel}
            none={spaTranslate("Anywhere busy")}
          />
        </Field>
      </Panel>

      <Panel title={spaTranslate("STORM SURGES")}>
        <ModuleOff guild={guild} modules={modules} module="party" />
        <SwitchRow
          title={spaTranslate("Timed storms")}
          control={
            <PixelToggle label={spaTranslate("Timed storms")} checked={surgeOn} onChange={setSurgeOn} />
          }
        >
          {" "}
          {spaTranslate(
            "A storm surge comes to a channel on a schedule: everyone stacks sandbags to hold the wall. Off until a server turns it on; managers can always call one with /surge.",
          )}{" "}
        </SwitchRow>
        <div className="form-grid form-grid-2">
          <Field
            label={spaTranslate("Storm channel")}
            htmlFor="surge-channel"
            error={surgeChanged ? fieldError("surge", "channel_id") : null}
            hint={surgeOn && !surgeChannel ? spaTranslate("Pick the channel storms come to.") : undefined}
          >
            <ChannelPicker
              id="surge-channel"
              info={discord.data}
              value={surgeChannel}
              onChange={setSurgeChannel}
            />
          </Field>
          <Field
            label={spaTranslate("How often")}
            htmlFor="surge-every"
            error={surgeChanged ? fieldError("surge", "every_hours") : null}
            hint={
              saved.surge.next_at
                ? spaTranslate("The next one comes {arg1}.", { arg1: dateTime(saved.surge.next_at) })
                : undefined
            }
          >
            <select
              id="surge-every"
              className="pixel-input pixel-select"
              value={surgeEvery}
              onChange={(event) => setSurgeEvery(Number(event.target.value))}
            >
              {saved.surge.every_choices.map((hours) => (
                <option key={hours} value={hours}>
                  {EVERY[hours]
                    ? spaTranslate(EVERY[hours])
                    : spaTranslate("Every {arg1} hours", { arg1: hours })}
                </option>
              ))}
            </select>
          </Field>
        </div>
      </Panel>

      <BirthdaySummary guild={guild} />

      <SaveBar
        count={changes}
        saving={save.isPending}
        error={save.isError && !(refused && refused.reason.field) ? save.error.message : null}
        onSave={submit}
        onReset={reset}
      />
      <Toast message={toast} onDone={clearToast} />
    </>
  );
}

function BirthdaySummary({ guild }: { guild: string }) {
  const birthdays = useBirthdays(guild);
  const data = birthdays.data;
  const t = birthdayWords(data?.strings, data?.language);
  return (
    <Panel title={t("BIRTHDAYS")} aside={<Link to={`/g/${guild}/birthdays`}>{t("Edit settings →")}</Link>}>
      <p className="panel-lead muted">
        {t(
          "Members choose whether to share their date. Birth years stay private; celebrations use each member’s local midnight.",
        )}
      </p>
      {birthdays.isPending && <Loading label={t("Reading birthdays")} />}
      {birthdays.isError && <Problem error={birthdays.error} />}
      {data && (
        <>
          <p>
            <strong>{data.settings.enabled ? t("Announcements on") : t("Announcements off")}</strong>
            {data.settings.channel_id
              ? t(" · Channel {id}", { id: data.settings.channel_id })
              : t(" · No channel")}
            {t(" · {count} shell gift", { count: data.settings.shells_gift })}
          </p>
          <p className="muted">
            {data.upcoming.length
              ? t("Next: {list}", {
                  list: data.upcoming
                    .slice(0, 3)
                    .map((b) => `${b.name} (${b.day}/${b.month})`)
                    .join(", "),
                })
              : t("No shared birthdays in the next 30 days.")}
          </p>
          <Link to={`/g/${guild}/birthdays`}>
            {t("Set the role, message and time zone; see all upcoming dates")}
          </Link>
        </>
      )}
    </Panel>
  );
}
