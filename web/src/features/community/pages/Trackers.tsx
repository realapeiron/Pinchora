import { useState } from "react";
import { Link, useOutletContext } from "react-router";
import { ChannelPicker } from "../../../components/Pickers";
import { Field } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { PixelButton } from "../../../components/PixelButton";
import { Toast } from "../../../components/Toast";
import { type Overview } from "../../../lib/api";
import {
  type TrackersData,
  type TrackersDraft,
  useTrackers,
  useSaveTrackers,
  trackersDraft,
  words,
} from "../api/trackers";
import { useDiscord } from "../../../lib/queries";
import { Loading, Problem } from "../../../components/States";
import { SettingsPage } from "../../../components/PageHead";
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
  const t = words(data);
  const discord = useDiscord(guild);
  const save = useSaveTrackers(guild);
  const [draft, setDraft] = useState<TrackersDraft>(() => trackersDraft(data));
  const [newHost, setNewHost] = useState("");
  const [newChannel, setNewChannel] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const changed = JSON.stringify(trackersDraft(draft)) !== JSON.stringify(trackersDraft(data));
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
      title={t("Game trackers")}
      lead={t("Follow official game updates and keep Minecraft servers in sight.")}
    >
      {!enabled && (
        <div className="notice notice-heart" role="note">
          {t("The Game trackers module is off.")} <Link to={`/g/${guild}/modules`}>{t("Open modules")}</Link>
        </div>
      )}
      <div className="trackers-grid">
        <Panel title={t("PATCH NOTES")} tone="accent">
          <p className="muted">
            {t("Official League, Valorant and Minecraft updates are posted once per release.")}
          </p>
          {games.map((game) => (
            <Field
              key={game}
              label={t(game === "lol" ? "League of Legends" : game === "mc" ? "Minecraft" : "Valorant")}
              htmlFor={`patch-${game}`}
            >
              <ChannelPicker
                id={`patch-${game}`}
                info={discord.data}
                value={draft.patch_channels[game] ?? null}
                none={t("No channel")}
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
          ))}
        </Panel>
        <Panel title={t("MINECRAFT WATCH LIST")} tone="gold">
          <p className="muted">{t("Status cards refresh every five minutes. Public Java servers only.")}</p>
          <ul className="trackers-watch-list">
            {draft.mc_watch.map((watch) => (
              <li key={watch.host}>
                <span>
                  <strong>{watch.host}</strong>
                  <small>
                    #{discord.data?.channels.find((c) => c.id === watch.channel_id)?.name ?? watch.channel_id}
                  </small>
                </span>
                <PixelButton
                  variant="ghost"
                  aria-label={`${t("Remove")} ${watch.host}`}
                  onClick={() =>
                    setDraft((old) => ({
                      ...old,
                      mc_watch: old.mc_watch.filter((item) => item.host !== watch.host),
                    }))
                  }
                >
                  {t("Remove")}
                </PixelButton>
              </li>
            ))}
          </ul>
          <div className="trackers-add">
            <Field label={t("Server address")} htmlFor="mc-host" hint={t("Example: play.example.com:25565")}>
              <input
                id="mc-host"
                className="pixel-input"
                value={newHost}
                maxLength={253}
                onChange={(e) => setNewHost(e.target.value)}
              />
            </Field>
            <Field label={t("Status channel")} htmlFor="mc-channel">
              <ChannelPicker
                id="mc-channel"
                info={discord.data}
                value={newChannel}
                none={t("Pick a channel")}
                onChange={setNewChannel}
              />
            </Field>
          </div>
          <PixelButton
            variant="secondary"
            disabled={!newHost.trim() || !newChannel || draft.mc_watch.length >= 20}
            onClick={addWatch}
          >
            {t("Add server")}
          </PixelButton>
        </Panel>
      </div>
      <Panel title={t("MINECRAFT NEWS")}>
        <Field label={t("Release channel")} htmlFor="mc-news">
          <ChannelPicker
            id="mc-news"
            info={discord.data}
            value={draft.mc_news_channel_id}
            none={t("No channel")}
            onChange={(channel) => setDraft((old) => ({ ...old, mc_news_channel_id: channel }))}
          />
        </Field>
      </Panel>
      <div className="trackers-actions">
        <PixelButton
          disabled={!changed || save.isPending}
          onClick={() =>
            save.mutate(draft, {
              onSuccess: (result) => {
                setDraft(trackersDraft(result));
                setNotice(t("Tracker settings saved."));
              },
              onError: (error) => setNotice(error.message),
            })
          }
        >
          {save.isPending ? t("Saving…") : t("Save changes")}
        </PixelButton>
        {changed && (
          <PixelButton
            variant="ghost"
            disabled={save.isPending}
            onClick={() => setDraft(trackersDraft(data))}
          >
            {t("Discard changes")}
          </PixelButton>
        )}
      </div>
      {notice && <Toast message={notice} onDone={() => setNotice(null)} />}
    </SettingsPage>
  );
}
