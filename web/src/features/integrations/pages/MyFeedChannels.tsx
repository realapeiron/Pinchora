import { useState } from "react";
import { serverText, t as spaT } from "../../../i18n";
import { FeedWordsContext, useFeedWords } from "../api/feedsWords";

import { Field } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { linkedChannelsSummary } from "./feedsSummary";
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
  const text = useFeedWords();
  const channels = useMyChannels();
  const start = useStartChannel();
  const verify = useVerifyChannel();
  const unlink = useUnlinkChannel();
  const [kind, setKind] = useState<"twitch" | "kick">("twitch");
  const [source, setSource] = useState("");
  const selectedKind = (list?.kinds ?? channels.data?.kinds)?.find((item) => item.key === kind);
  const content = (
    <>
      <p className="muted">{text("my-channels-help")}</p>
      {channels.data?.items.map((channel) => (
        <div key={channel.kind} className="tag-row feed-row">
          <div className="tag-main">
            <strong>
              {text(channel.kind)}: {channel.source}
            </strong>
            <p className="muted">
              {channel.verified_at ? text("channel-verified") : text("channel-pending")}
            </p>
            {channel.code && (
              <p>
                {text("put-code-in-title-or-bio")}: <code>{channel.code}</code>
              </p>
            )}
          </div>
          <div className="tag-actions">
            {!channel.verified_at && (
              <PixelButton disabled={verify.isPending} onClick={() => verify.mutate(channel.kind)}>
                {text("verify-channel")}
              </PixelButton>
            )}
            <PixelButton
              variant="ghost"
              disabled={unlink.isPending}
              onClick={() => unlink.mutate(channel.kind)}
            >
              {text("unlink-channel")}
            </PixelButton>
          </div>
        </div>
      ))}
      <div className="form-grid form-grid-2">
        <Field label={text("platform")} htmlFor="my-channel-kind">
          <select
            id="my-channel-kind"
            value={kind}
            onChange={(event) => setKind(event.target.value as "twitch" | "kick")}
          >
            <option value="twitch">{text("twitch")}</option>
            <option value="kick">{text("kick")}</option>
          </select>
        </Field>
        <Field label={text("channel-login")} htmlFor="my-channel-login">
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
        {text("link-channel")}
      </PixelButton>
      {[channels.error, start.error, verify.error, unlink.error].filter(Boolean).map((error, i) => (
        <p className="field-error" role="alert" key={i}>
          {serverText("error", { error: (error as Error).message })}
        </p>
      ))}
    </>
  );
  return list ? (
    <SettingsRow
      rowId="my-feed-channels-linked"
      title={text("my-channels")}
      description={dt("Manage your linked streaming channels.")}
      summary={linkedChannelsSummary(channels.data?.items.length ?? 0)}
    >
      {content}
    </SettingsRow>
  ) : (
    <Panel title={text("my-channels")}>{content}</Panel>
  );
}

/** No feed yet: what can be followed, each a way into the form. */
