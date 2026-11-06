import { useCallback, useState } from "react";
import { useOutletContext } from "react-router";

import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { ChannelPicker } from "../components/Pickers";
import { PixelToggle } from "../components/PixelToggle";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import { ApiError, type Community as CommunityData, type Overview } from "../lib/api";
import { useCommunity, useDiscord } from "../lib/queries";
import { Loading, Problem } from "./States";
import { PageHead } from "./PageHead";

export function Community() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const { query } = useCommunity(guild);
  return (
    <div className="settings-page">
      <PageHead
        title="COMMUNITY"
        lead="Where /suggest posts ideas, and whether critters wash up while people chat."
      />
      {query.isPending && <Loading label="Reading the settings" />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && <CommunityForm guild={guild} saved={query.data} />}
    </div>
  );
}

function CommunityForm({ guild, saved }: { guild: string; saved: CommunityData }) {
  const discord = useDiscord(guild);
  const { save } = useCommunity(guild);
  const [suggestions, setSuggestions] = useState(saved.suggestions.channel_id);
  const [enabled, setEnabled] = useState(saved.critters.enabled);
  const [critterChannel, setCritterChannel] = useState(saved.critters.channel_id);
  const [toast, setToast] = useState<string | null>(null);
  const clearToast = useCallback(() => setToast(null), []);
  const suggestionsChanged = suggestions !== saved.suggestions.channel_id;
  const crittersChanged = enabled !== saved.critters.enabled || critterChannel !== saved.critters.channel_id;
  const changes =
    Number(suggestionsChanged) +
    Number(enabled !== saved.critters.enabled) +
    Number(critterChannel !== saved.critters.channel_id);
  const fieldError = (field: string) =>
    save.error instanceof ApiError && save.error.field === field ? save.error.message : null;

  const submit = () =>
    save.mutate(
      {
        ...(suggestionsChanged ? { suggestions: { channel_id: suggestions } } : {}),
        ...(crittersChanged ? { critters: { enabled, channel_id: critterChannel } } : {}),
      },
      { onSuccess: () => setToast("Saved. The bot uses it from the next message.") },
    );
  const reset = () => {
    setSuggestions(saved.suggestions.channel_id);
    setEnabled(saved.critters.enabled);
    setCritterChannel(saved.critters.channel_id);
    save.reset();
  };

  return (
    <>
      <Panel title="SUGGESTIONS">
        <Field
          label="Suggestion channel"
          htmlFor="suggestion-channel"
          error={suggestionsChanged ? fieldError("channel_id") : null}
          hint="Members post ideas with /suggest; everyone votes with buttons, managers approve or deny."
        >
          <ChannelPicker
            id="suggestion-channel"
            info={discord.data}
            value={suggestions}
            onChange={setSuggestions}
            none="Off: /suggest is closed"
          />
        </Field>
      </Panel>

      <Panel title="CRITTER DROPS">
        <SwitchRow
          title="Chat drops"
          control={<PixelToggle label="Chat drops" checked={enabled} onChange={setEnabled} />}
        >
          While people chat, critters and message bottles now and then wash up for the quickest to grab. Off
          until a server turns it on.
        </SwitchRow>
        <Field
          label="Where they land"
          htmlFor="critter-channel"
          error={crittersChanged ? fieldError("channel_id") : null}
          hint="Only chat in a set channel counts towards a drop."
        >
          <ChannelPicker
            id="critter-channel"
            info={discord.data}
            value={critterChannel}
            onChange={setCritterChannel}
            none="Anywhere busy"
          />
        </Field>
      </Panel>

      <SaveBar
        count={changes}
        saving={save.isPending}
        error={save.isError && !fieldError("channel_id") ? save.error.message : null}
        onSave={submit}
        onReset={reset}
      />
      <Toast message={toast} onDone={clearToast} />
    </>
  );
}
