import { t as deferredText } from "../../../i18n/dashboard";
import { useRef, useState, type ChangeEvent } from "react";
import { translate as tr } from "../../../i18n";
import { Field } from "../../../components/Field";
import { SettingsRow } from "../../../components/SettingsRow";
import { PixelButton } from "../../../components/PixelButton";
import { RolePicker, RoleTag } from "../../../components/Pickers";
import type { GuildInfo } from "../../../lib/api";
import { readLevelImport, useLevelingPolicy } from "../api/levelingPolicy";
import { t } from "../../../i18n/dashboard";
import { levelingSummary } from "./levelingSummary";

type Data = ReturnType<typeof useLevelingPolicy>;
export function LevelingPolicyExtras({
  data,
  info,
  errorText,
}: {
  data: Data;
  info: GuildInfo | undefined;
  errorText: (message: string) => string;
}) {
  const [file, setFile] = useState("");
  const fileVersion = useRef(0);
  const previewVersion = useRef(0);
  const [format, setFormat] = useState<"json" | "csv">("json");
  const [mode, setMode] = useState<"merge" | "replace">("merge");
  const [previewId, setPreviewId] = useState<string | null>(null);
  const [fileError, setFileError] = useState<string | null>(null);
  const [voiceRole, setVoiceRole] = useState<string | null>(null);
  const [voiceRoleLevel, setVoiceRoleLevel] = useState(5);
  const readFile = async (event: ChangeEvent<HTMLInputElement>) => {
    const version = ++fileVersion.current;
    ++previewVersion.current;
    const picked = event.target.files?.[0];
    setPreviewId(null);
    setFile("");
    setFileError(null);
    if (!picked) return;
    try {
      const loaded = await readLevelImport(picked);
      if (version !== fileVersion.current) return;
      setFile(loaded.data);
      setFormat(loaded.format);
    } catch {
      if (version === fileVersion.current)
        setFileError(deferredText("File must be readable and at most 2 MB."));
    }
  };
  return (
    <>
      <SettingsRow
        rowId="leveling-policy-extras-voice-level-roles"
        title={t("Voice level roles", {}, "VOICE LEVEL ROLES")}
        description={t("Give roles for voice levels.")}
        summary={levelingSummary({}, { voiceRoles: data.voiceRoles.data?.items.length }).voiceRoles}
      >
        <div className="settings-detail-fields">
          <span className="pixel-label muted">{data.voiceRoles.data?.items.length ?? 0}/25</span>
          <p className="muted panel-lead">
            {deferredText("When separate voice levels are on, members earn these roles from voice time.")}
          </p>
          {data.voiceRoles.data?.items.length ? (
            <ul className="list">
              {data.voiceRoles.data.items.map((row) => (
                <li className="list-row" key={row.role_id}>
                  <span className="pixel-label">
                    {tr("Voice level")} {row.level}
                  </span>
                  <span className="list-main">
                    <RoleTag info={info} id={row.role_id} />
                  </span>
                  <PixelButton
                    variant="ghost"
                    disabled={data.deleteVoiceRole.isPending}
                    onClick={() => data.deleteVoiceRole.mutate(row.role_id)}
                  >
                    {tr("Remove")}
                  </PixelButton>
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted">{deferredText("No voice level roles yet.")}</p>
          )}
          <div className="add-row">
            <Field label={tr("Voice level")} htmlFor="voice-role-level">
              <input
                id="voice-role-level"
                className="pixel-input input-short"
                type="number"
                min={1}
                max={1000}
                value={voiceRoleLevel}
                onChange={(e) => setVoiceRoleLevel(Number(e.target.value))}
              />
            </Field>
            <Field label={tr("Role")} htmlFor="voice-role-pick">
              <RolePicker
                id="voice-role-pick"
                info={info}
                value={voiceRole}
                onChange={setVoiceRole}
                assignable
              />
            </Field>
            <PixelButton
              disabled={
                !voiceRole || voiceRoleLevel < 1 || voiceRoleLevel > 1000 || data.putVoiceRole.isPending
              }
              onClick={() =>
                voiceRole &&
                data.putVoiceRole.mutate(
                  { role: voiceRole, level: voiceRoleLevel },
                  { onSuccess: () => setVoiceRole(null) },
                )
              }
            >
              {tr("Add")}
            </PixelButton>
          </div>
          {data.putVoiceRole.isError && (
            <p className="field-error" role="alert">
              {errorText(data.putVoiceRole.error.message)}
            </p>
          )}
        </div>
      </SettingsRow>
      <SettingsRow
        rowId="leveling-policy-extras-top-gainers-this-week"
        title={t("Top gainers this week", {}, "TOP GAINERS THIS WEEK")}
        description={t("See who earned the most XP this week.")}
        summary={
          data.highlights.data?.items.length
            ? t("{count} members", { count: data.highlights.data.items.length })
            : t("No XP earned this week")
        }
      >
        <div className="settings-detail-fields">
          <p className="muted panel-lead">
            {deferredText(
              "The current UTC week starts Monday. Members with the most new server XP appear here.",
            )}
          </p>
          {data.highlights.data?.items.length ? (
            <ol>
              {data.highlights.data.items.map((row) => (
                <li key={row.user_id}>
                  {row.user_id}: {row.xp} XP
                </li>
              ))}
            </ol>
          ) : (
            <p className="muted">{tr("No XP earned this week yet.")}</p>
          )}
        </div>
      </SettingsRow>
      <SettingsRow
        rowId="leveling-policy-extras-import-levels-from-another-bot"
        title={t("Import levels from another bot", {}, "IMPORT LEVELS FROM ANOTHER BOT")}
        description={t("Import levels from another bot.")}
        summary={
          previewId
            ? t("{count} members", { count: data.preview.data?.count ?? 0 })
            : levelingSummary({}).import
        }
      >
        <div className="settings-detail-fields">
          <p className="muted panel-lead">
            {deferredText(
              "Upload an Arcane, Lurkr or Amari style JSON or CSV export. Preview the rows before applying them. Files stay private to this session for ten minutes.",
            )}
          </p>
          <div className="form-grid">
            <Field label={deferredText("Export file")} htmlFor="parity-file">
              <input
                id="parity-file"
                type="file"
                accept=".json,.csv,application/json,text/csv"
                onChange={readFile}
              />
            </Field>
            <Field label={deferredText("How to combine XP")} htmlFor="parity-mode">
              <select
                id="parity-mode"
                className="pixel-input pixel-select"
                value={mode}
                onChange={(e) => {
                  ++previewVersion.current;
                  setMode(e.target.value as typeof mode);
                  setPreviewId(null);
                }}
              >
                <option value="merge">{deferredText("Keep higher XP")}</option>
                <option value="replace">{deferredText("Use imported XP")}</option>
              </select>
            </Field>
          </div>
          {fileError && (
            <p role="alert" className="field-error">
              {fileError}
            </p>
          )}
          <PixelButton
            disabled={!file || data.preview.isPending}
            onClick={() => {
              const version = previewVersion.current;
              data.preview.mutate(
                { format, data: file, mode },
                {
                  onSuccess: (result) => {
                    if (version === previewVersion.current) setPreviewId(result.preview_id);
                  },
                },
              );
            }}
          >
            {deferredText("Preview import")}
          </PixelButton>
          {data.preview.isError && (
            <p role="alert" className="field-error">
              {errorText(data.preview.error.message)}
            </p>
          )}
          {previewId && data.preview.data && (
            <div>
              <p>
                {deferredText("Members in file")}: {data.preview.data.count} ·{" "}
                {deferredText("Members changing")}: {data.preview.data.changed}
              </p>
              <ol>
                {data.preview.data.sample.map((row) => (
                  <li key={row.user_id}>
                    {row.user_id}: {row.current_xp} → {row.result_xp} XP
                  </li>
                ))}
              </ol>
              <PixelButton
                disabled={data.apply.isPending}
                onClick={() => data.apply.mutate(previewId, { onSuccess: () => setPreviewId(null) })}
              >
                {deferredText("Apply import")}
              </PixelButton>
            </div>
          )}
          {data.apply.isSuccess && (
            <p role="status">
              {deferredText("Import complete")}: {data.apply.data.updated}/{data.apply.data.checked}
            </p>
          )}
          {data.apply.isError && (
            <p role="alert" className="field-error">
              {errorText(data.apply.error.message)}
            </p>
          )}
        </div>
      </SettingsRow>
    </>
  );
}
