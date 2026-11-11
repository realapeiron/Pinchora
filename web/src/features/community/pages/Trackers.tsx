import { useState } from "react";
import { Link, useOutletContext } from "react-router";
import { ChannelPicker } from "../../../components/Pickers";
import { Field } from "../../../components/Field";
import { PixelButton } from "../../../components/PixelButton";
import { Toast } from "../../../components/Toast";
import { type Overview } from "../../../lib/api";
import {
  type TrackersData,
  type TrackersDraft,
  useTrackers,
  useSaveTrackers,
  trackersDraft,
  words as serverWords,
} from "../api/trackers";
import { useDiscord } from "../../../lib/queries";
import { Loading, Problem } from "../../../components/States";
import { SettingsPage } from "../../../components/PageHead";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { trackersSummary, watchSummary } from "./trackersSummary";
import "../../../styles/trackers.css";

const games = ["lol", "valorant", "mc"] as const;
export function Trackers() {
  const overview = useOutletContext<Overview>();
  const guild = overview.guild.id;
  const query = useTrackers(guild);
  if (query.isPending) return <Loading />;
  if (query.isError) return <Problem error={query.error} />;
  return (
    <TrackersBody
      guild={guild}
      data={query.data}
      enabled={overview.modules.find((m) => m.key === "game_trackers")?.enabled !== false}
    />
  );
}
function TrackersBody({ guild, data, enabled }: { guild: string; data: TrackersData; enabled: boolean }) {
  const text = serverWords(data);
  const discord = useDiscord(guild);
  const save = useSaveTrackers(guild);
  const [draft, setDraft] = useState<TrackersDraft>(() => trackersDraft(data));
  const [newHost, setNewHost] = useState("");
  const [newChannel, setNewChannel] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const changed = JSON.stringify(trackersDraft(draft)) !== JSON.stringify(trackersDraft(data));
  const summary = trackersSummary(draft, discord.data);
  const addWatch = () => {
    const host = newHost.trim();
    if (
      !host ||
      !newChannel ||
      draft.mc_watch.some((item) => item.host === host) ||
      draft.mc_watch.length >= 20
    )
      return;
    setDraft((old) => ({ ...old, mc_watch: [...old.mc_watch, { host, channel_id: newChannel }] }));
    setNewHost("");
    setNewChannel(null);
  };
  return (
    <SettingsPage
      className="settings-page trackers-page"
      title={text("Game trackers")}
      lead={text("Follow official game updates and keep Minecraft servers in sight.")}
    >
      {!enabled && (
        <div className="notice notice-heart" role="note">
          {text("The Game trackers module is off.")}{" "}
          <Link to={`/g/${guild}/modules`}>{text("Open modules")}</Link>
        </div>
      )}
      <div className="settings-detail-list">
        {games.map((game) => (
          <SettingsRow
            rowId={`tracker-${game}`}
            key={game}
            title={text(game === "lol" ? "League of Legends" : game === "mc" ? "Minecraft" : "Valorant")}
            description={dt("Follow official updates for this game.")}
            summary={summary[game]}
          >
            <Field label={text("Release channel")} htmlFor={`patch-${game}`}>
              <ChannelPicker
                id={`patch-${game}`}
                info={discord.data}
                value={draft.patch_channels[game] ?? null}
                none={text("No channel")}
                onChange={(channel) =>
                  setDraft((old) => {
                    const patch_channels = { ...old.patch_channels };
                    if (channel) patch_channels[game] = channel;
                    else delete patch_channels[game];
                    return { ...old, patch_channels };
                  })
                }
              />
            </Field>
          </SettingsRow>
        ))}
        <SettingsRow
          rowId="trackers-minecraft-watch-list"
          title={dt("Minecraft watch list", {}, "MINECRAFT WATCH LIST")}
          description={dt("Watch Minecraft server status.")}
          summary={summary.watch}
        >
          <p className="muted">
            {text("Status cards refresh every five minutes. Public Java servers only.")}
          </p>
          <ul className="trackers-watch-list">
            {draft.mc_watch.map((watch) => (
              <li key={watch.host}>
                <SettingsRow
                  rowId={`watch-${watch.host}`}
                  title={watch.host}
                  description={dt("Review this server and its update channel.")}
                  summary={watchSummary(watch.channel_id, discord.data)}
                >
                  <PixelButton
                    variant="ghost"
                    aria-label={`${text("Remove")} ${watch.host}`}
                    onClick={() =>
                      setDraft((old) => ({
                        ...old,
                        mc_watch: old.mc_watch.filter((item) => item.host !== watch.host),
                      }))
                    }
                  >
                    {text("Remove")}
                  </PixelButton>
                </SettingsRow>
              </li>
            ))}
          </ul>
          <div className="trackers-add">
            <Field
              label={text("Server address")}
              htmlFor="mc-host"
              hint={text("Example: play.example.com:25565")}
            >
              <input
                id="mc-host"
                className="pixel-input"
                value={newHost}
                maxLength={253}
                onChange={(e) => setNewHost(e.target.value)}
              />
            </Field>
            <Field label={text("Status channel")} htmlFor="mc-channel">
              <ChannelPicker
                id="mc-channel"
                info={discord.data}
                value={newChannel}
                none={text("Pick a channel")}
                onChange={setNewChannel}
              />
            </Field>
          </div>
          <PixelButton
            variant="secondary"
            disabled={!newHost.trim() || !newChannel || draft.mc_watch.length >= 20}
            onClick={addWatch}
          >
            {text("Add server")}
          </PixelButton>
        </SettingsRow>
        <SettingsRow
          rowId="trackers-minecraft-news"
          title={dt("Minecraft news", {}, "MINECRAFT NEWS")}
          description={dt("Post Minecraft release news.")}
          summary={summary.news}
        >
          <Field label={text("Release channel")} htmlFor="mc-news">
            <ChannelPicker
              id="mc-news"
              info={discord.data}
              value={draft.mc_news_channel_id}
              none={text("No channel")}
              onChange={(channel) => setDraft((old) => ({ ...old, mc_news_channel_id: channel }))}
            />
          </Field>
        </SettingsRow>
      </div>
      <div className="trackers-actions">
        <PixelButton
          disabled={!changed || save.isPending}
          onClick={() =>
            save.mutate(draft, {
              onSuccess: (result) => {
                setDraft(trackersDraft(result));
                setNotice(text("Tracker settings saved."));
              },
              onError: (error) => setNotice(error.message),
            })
          }
        >
          {save.isPending ? text("Saving…") : text("Save changes")}
        </PixelButton>
        {changed && (
          <PixelButton
            variant="ghost"
            disabled={save.isPending}
            onClick={() => setDraft(trackersDraft(data))}
          >
            {text("Discard changes")}
          </PixelButton>
        )}
      </div>
      {notice && <Toast message={notice} onDone={() => setNotice(null)} />}
    </SettingsPage>
  );
}
