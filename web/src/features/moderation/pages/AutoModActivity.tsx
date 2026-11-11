import { dateTime } from "../../../i18n";
import { Field } from "../../../components/Field";
import { ChannelPicker } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { SettingsRow } from "../../../components/SettingsRow";
import { dashboardText } from "./automodSummary";
import type { GuildInfo } from "../../../lib/api";
import { actionLabel, type AutoConfig, type AutoData, type Words, type useAutoMod } from "../api/automod";

type Actions = ReturnType<typeof useAutoMod>;
export function AutoModTest({
  t,
  summary,
  info,
  test,
  text,
  setText,
  testChannel,
  setTestChannel,
  config,
}: {
  t: Words;
  summary: string;
  info: GuildInfo | undefined;
  test: Actions["test"];
  text: string;
  setText: (value: string) => void;
  testChannel: string | null;
  setTestChannel: (value: string | null) => void;
  config: AutoConfig;
}) {
  return (
    <SettingsRow
      title={t("ui.test")}
      description={dashboardText("Test rules without creating hits.")}
      summary={summary}
    >
      <div className="settings-detail-fields">
        <Field label={t("ui.test_channel")} htmlFor="am-test-channel">
          <ChannelPicker
            id="am-test-channel"
            info={info}
            value={testChannel}
            onChange={setTestChannel}
            none={t("ui.pick")}
          />
        </Field>
        <p className="muted panel-lead">{t("ui.test_hint")}</p>
        <Field label={t("ui.test_text")} htmlFor="am-test">
          <textarea
            id="am-test"
            className="pixel-input"
            rows={3}
            maxLength={2000}
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
        </Field>
        <PixelButton
          disabled={!text.trim() || test.isPending}
          onClick={() => test.mutate({ text, config, channel_id: testChannel })}
        >
          {t("ui.run_test")}
        </PixelButton>
      </div>
      <div className="settings-detail-preview">
        {!test.data && !test.isError && <p className="muted">{summary}</p>}
        {test.isError && (
          <p className="field-error" role="alert">
            {test.error.message}
          </p>
        )}
        {test.data && (
          <div className="automod-test-result" role="status">
            {test.data.matches.length === 0 ? (
              <p>{t("no_matches")}</p>
            ) : (
              <ul>
                {test.data.matches.map((r, i) => (
                  <li key={i}>
                    <strong>{t(`kind.${r.kind}`)}</strong> ·{" "}
                    {r.log_only
                      ? t("ui.dry_result")
                      : r.actions.map((a) => t(`action.${a.kind}`)).join(" → ")}
                  </li>
                ))}
              </ul>
            )}
            <p className="muted">{test.data.note}</p>
          </div>
        )}
      </div>
    </SettingsRow>
  );
}

export function AutoModHits({
  t,
  summary,
  saved,
  config,
  reset,
  setToast,
}: {
  t: Words;
  summary: string;
  saved: AutoData;
  config: AutoConfig;
  reset: Actions["reset"];
  setToast: (value: string | null) => void;
}) {
  return (
    <SettingsRow
      title={t("ui.hits")}
      description={dashboardText("Review recent rule hits.")}
      summary={summary}
    >
      <div className="settings-detail-fields">
        <p className="muted panel-lead">{t("ui.hits_hint")}</p>
        {saved.hits.length === 0 ? (
          <p className="automod-empty">{t("ui.empty_hits")}</p>
        ) : (
          <ul className="automod-hits">
            {saved.hits.map((h) => (
              <li key={h.id}>
                <div>
                  <strong>{config.rules.find((r) => r.id === h.rule_id)?.name ?? `#${h.id}`}</strong>
                  <time dateTime={h.created_at}>{dateTime(h.created_at)}</time>
                </div>
                <p>
                  {t("ui.member")}: <code>{h.user_id}</code> · <code>#{h.channel_id}</code>
                </p>
                <p>{actionLabel(h.action, t, saved.actions)}</p>
                {h.snippet && <blockquote>{h.snippet}</blockquote>}
                <PixelButton
                  variant="secondary"
                  disabled={reset.isPending}
                  onClick={() => reset.mutate(h.user_id, { onSuccess: () => setToast(t("ui.saved")) })}
                >
                  {t("ui.reset")}
                </PixelButton>
              </li>
            ))}
          </ul>
        )}
        {reset.isError && <p role="alert">{reset.error.message}</p>}
      </div>
    </SettingsRow>
  );
}
