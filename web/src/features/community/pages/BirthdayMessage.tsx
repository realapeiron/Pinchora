import { useState } from "react";
import { Field } from "../../../components/Field";
import { PixelButton } from "../../../components/PixelButton";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import type { BirthdaySettings } from "../api/birthdays";

type Words = (key: string, args?: Record<string, string | number>) => string;
type Change = <K extends keyof BirthdaySettings>(key: K, value: BirthdaySettings[K]) => void;

export function BirthdayMessage({
  draft,
  change,
  fieldError,
  t,
  summary,
}: {
  draft: BirthdaySettings;
  change: Change;
  fieldError: (key: string) => string | undefined;
  t: Words;
  summary: string;
}) {
  const [preview, setPreview] = useState(false);
  const sample = draft.message.replaceAll("{user}", "@Mira").replaceAll("{age}", draft.allow_age ? "24" : "");
  return (
    <SettingsRow
      rowId="birthday-message-the-message"
      title={dt("The message", {}, "THE MESSAGE")}
      description={dt("Write the birthday card message.")}
      summary={summary}
    >
      <div className="settings-detail-fields">
        <Field
          label={t("Birthday message")}
          htmlFor="bd-message"
          error={fieldError("message")}
          hint={t("Use {user} for the member and {age} only when age sharing is allowed.")}
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
        <span className="pixel-label muted">{t("{count}/500 used", { count: draft.message.length })}</span>
      </div>
      <div className="settings-detail-preview">
        <PixelButton variant="secondary" onClick={() => setPreview((v) => !v)}>
          {preview ? t("Hide preview") : t("Preview card")}
        </PixelButton>
        {preview && (
          <div className="birthday-preview" role="img" aria-label={t("Birthday card preview")}>
            <div className="birthday-cake" aria-hidden="true">
              <i />
              <b />
              <strong />
            </div>
            <div>
              <span className="pixel-label">{t("HAPPY BIRTHDAY")}</span>
              <h3>Mira</h3>
              <p>{sample}</p>
              {draft.badge && <span className="birthday-ribbon">{t("✦ CAKE DAY")}</span>}
            </div>
          </div>
        )}
      </div>
    </SettingsRow>
  );
}
