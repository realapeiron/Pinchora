import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { Field } from "../../../../components/Field";
import { type WelcomeModel } from "../../hooks/useWelcomeEditor";
import { ChannelPicker } from "../../../../components/DashboardPickers";
import { PreviewFrame } from "../../components/PreviewFrame";

export function TryIt({
  showBot,
  setShowBot,
  discord,
  testChannel,
  setTestChannel,
  sendTest,
  testing,
  local,
  guild,
  design,
  kind,
  side,
}: Pick<
  WelcomeModel,
  | "showBot"
  | "setShowBot"
  | "discord"
  | "testChannel"
  | "setTestChannel"
  | "sendTest"
  | "testing"
  | "local"
  | "guild"
  | "design"
  | "kind"
  | "side"
>) {
  return (
    <Panel
      title={deferredText("TRY IT")}
      aside={
        <PixelButton
          variant="secondary"
          className="button-small"
          aria-expanded={showBot}
          aria-controls="card-bot-preview"
          onClick={() => setShowBot((on) => !on)}
        >
          {showBot ? deferredText("Hide the bot's picture") : deferredText("Show the bot's picture")}
        </PixelButton>
      }
    >
      <div className="card-test">
        <Field
          label={deferredText("Post a test in")}
          htmlFor="card-test-channel"
          hint={deferredText("With you as the member, pinging nobody.")}
        >
          <ChannelPicker
            posting
            id="card-test-channel"
            info={discord.data}
            value={testChannel}
            onChange={setTestChannel}
          />
        </Field>
        <PixelButton
          onClick={() => void sendTest(false)}
          disabled={!testChannel || testing || local.length > 0}
        >
          {testing ? spaTranslate("Sending…") : spaTranslate("Send test")}
        </PixelButton>
        <PixelButton
          variant="secondary"
          onClick={() => void sendTest(true)}
          disabled={testing || local.length > 0}
        >
          {deferredText("Send test to my DMs")}
        </PixelButton>
      </div>
      {showBot && (
        <div className="card-bot-preview" id="card-bot-preview">
          <h3 className="pixel-label">{deferredText("WHAT THE BOT POSTS")}</h3>
          <PreviewFrame
            path={`/api/guilds/${guild}/preview/card`}
            body={local.length === 0 ? { design } : null}
            alt={`The ${kind} image the bot draws`}
            width={design.width}
            height={design.height}
            delay={1500}
          />
          <p className="muted fine">
            {" "}
            {deferredText(
              "Drawn by the bot with you as the member, from the design above, at most twenty times a minute for the whole server.",
            )}{" "}
            {side.custom
              ? ""
              : deferredText("This is the classic card from /setup welcome card until you change it.")}
          </p>
        </div>
      )}
    </Panel>
  );
}
