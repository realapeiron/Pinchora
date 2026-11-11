import { translate as spaTranslate } from "../../../i18n";
import { useOutletContext } from "react-router";

import { Field } from "../../../components/Field";
import { SettingsRow } from "../../../components/SettingsRow";
import { SaveBar } from "../../../components/SaveBar";
import { Toast } from "../../../components/Toast";
import type { FieldError, Overview, Settings } from "../../../lib/api";
import { LIMITS, checkPoints, checkPointsEmoji, checkPointsName } from "../../../lib/rules";
import { useSettingsForm } from "../../../lib/useSettingsForm";
import { SettingsPage } from "../../../components/PageHead";
import { t } from "../../../i18n/dashboard";
import { pointsSummary } from "./pointsSummary";
import { StoreItems } from "./StoreItems";

// Retained catalogue copy: Off keeps balances and the store; chatting just stops earning more.

function check(value: Settings): (FieldError | null)[] {
  const out = [checkPointsName(value.points_name), checkPointsEmoji(value.points_emoji)];
  const range = checkPoints(value.points_min, value.points_max);
  return [...out, range].map((c) => (c.ok ? null : c));
}

export function Points() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const form = useSettingsForm(guild, data.settings, check);
  const { value, set, errors } = form;
  const number = (raw: string) => (raw === "" ? Number.NaN : Number(raw));
  const summary = pointsSummary(value);

  return (
    <SettingsPage
      className="settings-page"
      title={spaTranslate("POINTS")}
      lead={spaTranslate(
        "A currency for this server only: members earn it by chatting and spend it on roles in /store.",
      )}
    >
      <div className="settings-detail-list">
        <SettingsRow
          title={spaTranslate("SERVER POINTS")}
          description={t("Set the name, emoji and points per message.")}
          summary={summary.earning}
          initialOpen
        >
          <div className="settings-detail-fields">
            <div className="form-grid form-grid-2">
              <Field
                label={spaTranslate("Name")}
                htmlFor="points-name"
                error={errors.points_name}
                hint={spaTranslate("What members see, e.g. coins or shells.")}
              >
                <input
                  id="points-name"
                  className="pixel-input"
                  value={value.points_name}
                  maxLength={LIMITS.points_name_max + 8}
                  onChange={(event) => set("points_name", event.target.value)}
                  aria-invalid={errors.points_name ? true : undefined}
                />
              </Field>
              <Field
                label={spaTranslate("Emoji")}
                htmlFor="points-emoji"
                error={errors.points_emoji}
                hint={spaTranslate("One emoji, or a custom one as <:name:id>.")}
              >
                <input
                  id="points-emoji"
                  className="pixel-input input-short"
                  value={value.points_emoji}
                  onChange={(event) => set("points_emoji", event.target.value)}
                  aria-invalid={errors.points_emoji ? true : undefined}
                />
              </Field>
              <Field label={spaTranslate("Least per message")} htmlFor="points-min" error={errors.points_min}>
                <input
                  id="points-min"
                  className="pixel-input input-short"
                  type="number"
                  min={LIMITS.points[0]}
                  max={LIMITS.points[1]}
                  value={Number.isNaN(value.points_min) ? "" : value.points_min}
                  onChange={(event) => set("points_min", number(event.target.value))}
                  aria-invalid={errors.points_min ? true : undefined}
                />
              </Field>
              <Field label={spaTranslate("Most per message")} htmlFor="points-max" error={errors.points_max}>
                <input
                  id="points-max"
                  className="pixel-input input-short"
                  type="number"
                  min={LIMITS.points[0]}
                  max={LIMITS.points[1]}
                  value={Number.isNaN(value.points_max) ? "" : value.points_max}
                  onChange={(event) => set("points_max", number(event.target.value))}
                  aria-invalid={errors.points_max ? true : undefined}
                />
              </Field>
            </div>
            <p className="sample-line">
              <span className="pixel-label muted">{spaTranslate("LOOKS LIKE")}</span>
              <span>
                {" "}
                {spaTranslate("Each message earns")}{" "}
                <strong>
                  {value.points_min}–{value.points_max}
                </strong>{" "}
                {value.points_emoji} {value.points_name}
                {spaTranslate(". XP multipliers scale it too.")}{" "}
              </span>
            </p>
          </div>
        </SettingsRow>

        <StoreItems
          guild={guild}
          emoji={data.settings.points_emoji}
          name={data.settings.points_name}
          settings={value}
        />
      </div>

      <SaveBar
        count={form.changes}
        saving={form.saving}
        blocked={form.blocked}
        error={form.barError}
        onSave={form.submit}
        onReset={form.reset}
      />
      <Toast
        message={form.toast ? t(form.toast) : null}
        onDone={form.clearToast}
        action={form.undo ? { label: spaTranslate("Undo"), onClick: form.undo } : undefined}
      />
    </SettingsPage>
  );
}
