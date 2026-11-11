import { useState, type Dispatch, type SetStateAction } from "react";
import { number } from "../../../i18n";
import { Field } from "../../../components/Field";
import { ChannelPicker, RolePicker } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { PixelToggle } from "../../../components/PixelToggle";
import { AtlasThumb } from "../../../components/card/CardProperties";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { ApiError, type GuildInfo } from "../../../lib/api";
import { useAtlasIndex } from "../../../lib/queries";
import { useAchievements, type AchievementDraft, type AchievementPage } from "../api/achievements";
import { KIND_LABELS, KIND_MODULES, SYMBOLS } from "./achievementLabels";
import { achievementSummary } from "./achievementsSummary";
import { AchievementPreview } from "./AchievementPresentation";

export function AchievementEditor({
  editing,
  draft,
  setDraft,
  data,
  discord,
  atlas,
  save,
  remove,
  modules,
  onSubmit,
  onCancel,
  onRemove,
  t: text,
}: {
  editing: string;
  draft: AchievementDraft;
  setDraft: Dispatch<SetStateAction<AchievementDraft>>;
  data: AchievementPage;
  discord: GuildInfo | undefined;
  atlas: ReturnType<typeof useAtlasIndex>["data"];
  save: ReturnType<typeof useAchievements>["save"];
  remove: ReturnType<typeof useAchievements>["remove"];
  modules: { key: string; enabled: boolean }[];
  onSubmit: () => void;
  onCancel: () => void;
  onRemove: (id: string) => void;
  t: (key: string) => string;
}) {
  const [preview, setPreview] = useState(false);
  const error = save.error instanceof ApiError ? save.error : null;
  const fieldError = (field: string) => (error?.field === field ? error.message : null);
  const off =
    !!KIND_MODULES[draft.kind] && modules.some((m) => m.key === KIND_MODULES[draft.kind] && !m.enabled);
  const badgeIcons = Object.keys(atlas ?? {})
    .filter((id) => id.startsWith("b_"))
    .slice(0, 80);
  const submit = onSubmit;
  const removeItem = onRemove;
  const setEditing = onCancel;
  return (
    <SettingsRow
      rowId="achievement-editor-form"
      title={editing === "new" ? text("Add achievement") : draft.name}
      description={dt("Manage milestones and rewards.")}
      summary={achievementSummary(draft)}
      initialOpen
    >
      <div className="ach-editor-grid">
        <div className="ach-editor-fields">
          <Field label={text("Name")} htmlFor="ach-name" error={fieldError("name")}>
            <input
              id="ach-name"
              className="pixel-input"
              maxLength={40}
              value={draft.name}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
            />
          </Field>
          <Field label={text("Description")} htmlFor="ach-desc" error={fieldError("description")}>
            <input
              id="ach-desc"
              className="pixel-input"
              maxLength={100}
              value={draft.description}
              onChange={(e) => setDraft({ ...draft, description: e.target.value })}
            />
          </Field>
          <div className="ach-two-fields">
            <Field label={text("Kind")} htmlFor="ach-kind" error={fieldError("kind")}>
              <select
                id="ach-kind"
                className="pixel-input pixel-select"
                value={draft.kind}
                onChange={(e) => setDraft({ ...draft, kind: e.target.value })}
              >
                {data.kinds.map((kind) => (
                  <option key={kind} value={kind}>
                    {text(KIND_LABELS[kind] ?? kind)}
                  </option>
                ))}
              </select>
            </Field>
            <Field label={text("Target")} htmlFor="ach-target" error={fieldError("target")}>
              <input
                id="ach-target"
                className="pixel-input"
                type="number"
                min={1}
                max={1000000000}
                value={draft.target}
                disabled={draft.kind === "custom"}
                onChange={(e) => setDraft({ ...draft, target: Number(e.target.value) })}
              />
            </Field>
          </div>
          {off && (
            <p className="notice">{text("This kind's module is off, so members will not see or earn it.")}</p>
          )}
          <Field label={text("Icon")} error={fieldError("icon")}>
            <div className="ach-icons" role="group" aria-label={text("Select an icon")}>
              {[...data.icons, ...badgeIcons].map((icon) => (
                <button
                  key={icon}
                  type="button"
                  className={`ach-icon ${draft.icon === icon ? "selected" : ""}`}
                  title={icon}
                  aria-label={icon}
                  aria-pressed={draft.icon === icon}
                  onClick={() => setDraft({ ...draft, icon })}
                >
                  {icon.startsWith("b_") ? (
                    <AtlasThumb atlas={atlas} id={icon} size={32} />
                  ) : (
                    <span>{SYMBOLS[icon] ?? "★"}</span>
                  )}
                </button>
              ))}
            </div>
          </Field>
          <div className="ach-two-fields">
            <Field label={text("Role reward")} htmlFor="ach-role" error={fieldError("reward_role_id")}>
              <RolePicker
                id="ach-role"
                info={discord}
                value={draft.reward_role_id}
                onChange={(v) => setDraft({ ...draft, reward_role_id: v })}
                none={text("No role")}
                assignable
                forActions
              />
            </Field>
            <Field label={text("Points reward")} htmlFor="ach-points" error={fieldError("reward_points")}>
              <input
                id="ach-points"
                className="pixel-input"
                type="number"
                min={0}
                max={1000000000}
                value={draft.reward_points}
                onChange={(e) => setDraft({ ...draft, reward_points: Number(e.target.value) })}
              />
            </Field>
          </div>
          <Field label={text("Announce in")} htmlFor="ach-channel" error={fieldError("announce_channel_id")}>
            <ChannelPicker
              id="ach-channel"
              info={discord}
              value={draft.announce_channel_id}
              onChange={(v) => setDraft({ ...draft, announce_channel_id: v })}
              none={text("No channel")}
            />
          </Field>
          <div className="ach-switches">
            <label>
              {text("Secret until earned")}
              <PixelToggle
                label={text("Secret until earned")}
                checked={draft.secret}
                onChange={(v) => setDraft({ ...draft, secret: v })}
              />
            </label>
            <label>
              {text("Enabled")}
              <PixelToggle
                label={text("Enabled")}
                checked={draft.enabled}
                onChange={(v) => setDraft({ ...draft, enabled: v })}
              />
            </label>
          </div>
          <div className="ach-actions">
            <PixelButton onClick={submit} disabled={save.isPending || !draft.name.trim() || draft.target < 1}>
              {text("Save")}
            </PixelButton>
            <PixelButton variant="secondary" onClick={() => setPreview(!preview)}>
              {text("Test preview")}
            </PixelButton>
            <PixelButton variant="ghost" onClick={() => setEditing()}>
              {text("Cancel")}
            </PixelButton>
            {editing !== "new" && (
              <PixelButton variant="ghost" onClick={() => removeItem(editing)} disabled={remove.isPending}>
                {text("Delete")}
              </PixelButton>
            )}
          </div>
          {save.isError && !error?.field && (
            <p className="field-error" role="alert">
              {save.error.message}
            </p>
          )}
        </div>
        <div className="ach-live settings-detail-preview">
          <span className="pixel-label">{text("Preview")}</span>
          <AchievementPreview item={draft} t={text} atlas={atlas} earned={preview} number={number} />
          <p className="muted">
            {draft.secret ? text("Secret until earned") : text(KIND_LABELS[draft.kind] ?? draft.kind)} ·{" "}
            {number(draft.target)}
          </p>
        </div>
      </div>
    </SettingsRow>
  );
}
