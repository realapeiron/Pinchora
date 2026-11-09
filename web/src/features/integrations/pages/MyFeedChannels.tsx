import { useState } from "react";
import { serverText, t as spaT } from "../../../i18n";
import { FeedWordsContext, useFeedWords } from "../api/feedsWords";

import { Field } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { PixelButton } from "../../../components/PixelButton";
import { TopBar } from "../../../components/TopBar";
import { type Me } from "../../../lib/api";
import { PageHead } from "../../../components/PageHead";
import { RequireLogin } from "../../../components/RequireLogin";
import { Loading, Problem } from "../../../components/States";
import {
  useMyChannels,
  useStartChannel,
  useUnlinkChannel,
  useVerifyChannel,
  type FeedList,
} from "../api/feeds";

export function MyFeedsChannels({ me }: { me: Me }) {
  const channels = useMyChannels();
  if (channels.isPending) return <Loading />;
  if (channels.isError) return <Problem error={channels.error} />;
  return (
    <div className="page">
      <TopBar me={me} />
      <main className="content content-narrow settings-page">
        <FeedWordsContext.Provider value={channels.data.strings}>
          <PageHead
            title={spaT("MY CHANNELS")}
            lead={spaT(
              "Link your Twitch or Kick channel to receive a Live now role in servers that follow you.",
            )}
          />
          <MyChannels />
        </FeedWordsContext.Provider>
      </main>
    </div>
  );
}

export function MyFeedsChannelsRoute() {
  return <RequireLogin>{(me) => <MyFeedsChannels me={me} />}</RequireLogin>;
}

export function MyChannels({ list }: { list?: FeedList }) {
  const t = useFeedWords();
  const channels = useMyChannels();
  const start = useStartChannel();
  const verify = useVerifyChannel();
  const unlink = useUnlinkChannel();
  const [kind, setKind] = useState<"twitch" | "kick">("twitch");
  const [source, setSource] = useState("");
  const selectedKind = (list?.kinds ?? channels.data?.kinds)?.find((item) => item.key === kind);
  return (
    <Panel title={t("my-channels")}>
      <p className="muted">{t("my-channels-help")}</p>
      {channels.data?.items.map((channel) => (
        <div key={channel.kind} className="tag-row feed-row">
          <div className="tag-main">
            <strong>
              {t(channel.kind)}: {channel.source}
            </strong>
            <p className="muted">{channel.verified_at ? t("channel-verified") : t("channel-pending")}</p>
            {channel.code && (
              <p>
                {t("put-code-in-title-or-bio")}: <code>{channel.code}</code>
              </p>
            )}
          </div>
          <div className="tag-actions">
            {!channel.verified_at && (
              <PixelButton disabled={verify.isPending} onClick={() => verify.mutate(channel.kind)}>
                {t("verify-channel")}
              </PixelButton>
            )}
            <PixelButton
              variant="ghost"
              disabled={unlink.isPending}
              onClick={() => unlink.mutate(channel.kind)}
            >
              {t("unlink-channel")}
            </PixelButton>
          </div>
        </div>
      ))}
      <div className="form-grid form-grid-2">
        <Field label={t("platform")} htmlFor="my-channel-kind">
          <select
            id="my-channel-kind"
            value={kind}
            onChange={(event) => setKind(event.target.value as "twitch" | "kick")}
          >
            <option value="twitch">{t("twitch")}</option>
            <option value="kick">{t("kick")}</option>
          </select>
        </Field>
        <Field label={t("channel-login")} htmlFor="my-channel-login">
          <input id="my-channel-login" value={source} onChange={(event) => setSource(event.target.value)} />
        </Field>
      </div>
      {!selectedKind?.available && (
        <p className="field-hint muted">
          {selectedKind?.why_not && serverText("unavailable", { unavailable: selectedKind.why_not })}
        </p>
      )}
      <PixelButton
        disabled={!source.trim() || start.isPending || !selectedKind?.available}
        onClick={() => start.mutate({ kind, source })}
      >
        {t("link-channel")}
      </PixelButton>
      {[channels.error, start.error, verify.error, unlink.error].filter(Boolean).map((error, i) => (
        <p className="field-error" role="alert" key={i}>
          {serverText("error", { error: (error as Error).message })}
        </p>
      ))}
    </Panel>
  );
}

/** No feed yet: what can be followed, each a way into the form. */
