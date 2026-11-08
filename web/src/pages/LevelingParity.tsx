import { useRef, useState, type ChangeEvent } from "react";
import { translate as tr } from "../i18n";
import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { ChannelPicker, RolePicker, RoleTag } from "../components/Pickers";
import type { GuildInfo } from "../lib/api";
import { useMe } from "../lib/queries";
import { type Curve, type LevelingParity, readLevelImport, useLevelingParity } from "../lib/levelingParity";

const apiErrors: Record<string, () => string> = {
  "Leave grace must be at most 604800 seconds.": () => tr("Leave grace must be at most 604800 seconds."),
  "Import members must be verified in this server.": () =>
    tr("Import members must be verified in this server."),
  "Malformed CSV row.": () => tr("Malformed CSV row."),
  "Invalid role ID.": () => tr("Invalid role ID."),
  "A server can have 25 voice level roles.": () => tr("A server can have 25 voice level roles."),
  "Invalid highlights channel.": () => tr("Invalid highlights channel."),
  "This leaderboard URL name is already in use.": () => tr("This leaderboard URL name is already in use."),
  "Invalid JSON import.": () => tr("Invalid JSON import."),
  "JSON must contain a players, members, leaderboard or users array.": () =>
    tr("JSON must contain a players, members, leaderboard or users array."),
  "A row has no valid Discord user ID.": () => tr("A row has no valid Discord user ID."),
  "A row has no XP total.": () => tr("A row has no XP total."),
  "Unclosed CSV quote.": () => tr("Unclosed CSV quote."),
  "CSV is empty.": () => tr("CSV is empty."),
  "CSV needs an ID column.": () => tr("CSV needs an ID column."),
  "CSV needs an XP column.": () => tr("CSV needs an XP column."),
  "A CSV row has an invalid Discord user ID.": () => tr("A CSV row has an invalid Discord user ID."),
  "A CSV row has invalid XP.": () => tr("A CSV row has invalid XP."),
  "Import file exceeds 2 MB.": () => tr("Import file exceeds 2 MB."),
  "Choose JSON or CSV.": () => tr("Choose JSON or CSV."),
  "Import must contain 1 to 10,000 members.": () => tr("Import must contain 1 to 10,000 members."),
  "XP must be 0 to 1,000,000,000.": () => tr("XP must be 0 to 1,000,000,000."),
  "Choose merge or replace.": () => tr("Choose merge or replace."),
  "Import cannot be saved.": () => tr("Import cannot be saved."),
  "Invalid preview ID.": () => tr("Invalid preview ID."),
  "Preview expired. Preview the file again.": () => tr("Preview expired. Preview the file again."),
  "Invalid preview.": () => tr("Invalid preview."),
  "Message XP must be between 1 and 1000, with minimum at most maximum.": () =>
    tr("Message XP must be between 1 and 1000, with minimum at most maximum."),
  "Message cooldown must be 5 to 3600 seconds.": () => tr("Message cooldown must be 5 to 3600 seconds."),
  "Bonus XP must be 1 to 100.": () => tr("Bonus XP must be 1 to 100."),
  "Curve coefficients are outside the supported range.": () =>
    tr("Curve coefficients are outside the supported range."),
  "Vanity URL must be 3 to 32 lowercase letters, digits or hyphens.": () =>
    tr("Vanity URL must be 3 to 32 lowercase letters, digits or hyphens."),
};
function apiError(message: string): string {
  return apiErrors[message]?.() ?? message;
}

function curveKind(curve: Curve): string {
  return typeof curve === "string" ? curve : "linear" in curve ? "linear" : "custom";
}
function chooseCurve(kind: string): Curve {
  switch (kind) {
    case "linear":
      return { linear: { base: 100, per_level: 50 } };
    case "custom":
      return { polynomial: { base: 100, linear: 50, quadratic: 5 } };
    case "mee6":
      return "mee6";
    default:
      return "classic";
  }
}

export function LevelingParityPanel({ guild, info }: { guild: string; info: GuildInfo | undefined }) {
  const data = useLevelingParity(guild);
  const me = useMe();
  if (data.query.isPending)
    return (
      <Panel title={tr("XP OPTIONS")}>
        <p>{tr("Loading XP options…")}</p>
      </Panel>
    );
  if (data.query.isError)
    return (
      <Panel title={tr("XP OPTIONS")}>
        <p role="alert">{apiError(data.query.error.message)}</p>
      </Panel>
    );
  return (
    <Editor
      key={JSON.stringify(data.query.data)}
      guild={guild}
      saved={data.query.data}
      data={data}
      info={info}
      messageContent={me.data?.capabilities.message_content ?? false}
    />
  );
}

type Data = ReturnType<typeof useLevelingParity>;
function Editor({
  guild,
  saved,
  data,
  info,
  messageContent,
}: {
  guild: string;
  saved: LevelingParity;
  data: Data;
  info: GuildInfo | undefined;
  messageContent: boolean;
}) {
  const [draft, setDraft] = useState(saved);
  const [lastSlug, setLastSlug] = useState(saved.vanity_slug ?? "");
  const [file, setFile] = useState("");
  const fileVersion = useRef(0);
  const previewVersion = useRef(0);
  const [format, setFormat] = useState<"json" | "csv">("json");
  const [mode, setMode] = useState<"merge" | "replace">("merge");
  const [previewId, setPreviewId] = useState<string | null>(null);
  const [fileError, setFileError] = useState<string | null>(null);
  const [voiceRole, setVoiceRole] = useState<string | null>(null);
  const [voiceRoleLevel, setVoiceRoleLevel] = useState(5);
  const set = <K extends keyof LevelingParity>(key: K, value: LevelingParity[K]) =>
    setDraft((old) => ({ ...old, [key]: value }));
  const number = (
    key: keyof Pick<
      LevelingParity,
      | "message_min"
      | "message_max"
      | "message_cooldown_secs"
      | "reaction_xp"
      | "effort_bonus"
      | "vote_xp"
      | "leave_grace_secs"
    >,
    label: string,
    min: number,
    max: number,
  ) => (
    <Field label={tr(label)} htmlFor={`parity-${key}`}>
      <input
        id={`parity-${key}`}
        className="pixel-input input-short"
        type="number"
        min={min}
        max={max}
        value={draft[key]}
        onChange={(e) => set(key, Number(e.target.value))}
      />
    </Field>
  );
  const toggle = (
    key: keyof Pick<
      LevelingParity,
      "messages" | "reactions" | "voice" | "effort" | "votes" | "separate_voice_level" | "reset_on_leave"
    >,
    label: string,
  ) => (
    <label className="list-row">
      <span className="list-main">{tr(label)}</span>
      <PixelToggle label={tr(label)} checked={draft[key]} onChange={(on) => set(key, on)} />
    </label>
  );
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
      if (version === fileVersion.current) setFileError(tr("File must be readable and at most 2 MB."));
    }
  };
  const kind = curveKind(draft.curve);
  const coeff =
    kind === "linear"
      ? (draft.curve as { linear: { base: number; per_level: number } }).linear
      : kind === "custom"
        ? (draft.curve as { polynomial: { base: number; linear: number; quadratic: number } }).polynomial
        : null;
  const invalid =
    draft.leave_grace_secs < 0 ||
    draft.leave_grace_secs > 604800 ||
    draft.message_min < 1 ||
    draft.message_max < draft.message_min ||
    draft.message_max > 1000 ||
    draft.message_cooldown_secs < 5 ||
    draft.message_cooldown_secs > 3600 ||
    [draft.reaction_xp, draft.effort_bonus, draft.vote_xp].some((v) => v < 1 || v > 100) ||
    (draft.vanity_slug !== null && !/^[a-z0-9](?:[a-z0-9-]{1,30})[a-z0-9]$/.test(draft.vanity_slug));
  return (
    <>
      <Panel title={tr("XP OPTIONS")}>
        <p className="muted panel-lead">
          {tr(
            "Choose how members earn levels. Changes to the curve recalculate displayed levels from existing XP.",
          )}
        </p>
        <Field label={tr("Level curve")} htmlFor="parity-curve">
          <select
            id="parity-curve"
            className="pixel-input pixel-select"
            value={kind}
            onChange={(e) => set("curve", chooseCurve(e.target.value))}
          >
            <option value="classic">{tr("Classic")}</option>
            <option value="mee6">{tr("MEE6 style")}</option>
            <option value="linear">{tr("Linear")}</option>
            <option value="custom">{tr("Custom polynomial")}</option>
          </select>
        </Field>
        {coeff && (
          <div className="form-grid">
            <Field label={tr("Base XP")} htmlFor="parity-base">
              <input
                id="parity-base"
                className="pixel-input input-short"
                type="number"
                min={10}
                max={100000}
                value={coeff.base}
                onChange={(e) =>
                  set(
                    "curve",
                    kind === "linear"
                      ? ({ linear: { ...coeff, base: Number(e.target.value) } } as Curve)
                      : ({ polynomial: { ...coeff, base: Number(e.target.value) } } as Curve),
                  )
                }
              />
            </Field>
            <Field label={tr("Linear coefficient")} htmlFor="parity-linear">
              <input
                id="parity-linear"
                className="pixel-input input-short"
                type="number"
                min={0}
                max={10000}
                value={"per_level" in coeff ? coeff.per_level : coeff.linear}
                onChange={(e) =>
                  set(
                    "curve",
                    kind === "linear"
                      ? ({ linear: { ...coeff, per_level: Number(e.target.value) } } as Curve)
                      : ({ polynomial: { ...coeff, linear: Number(e.target.value) } } as Curve),
                  )
                }
              />
            </Field>
            {kind === "custom" && (
              <Field label={tr("Quadratic coefficient")} htmlFor="parity-quadratic">
                <input
                  id="parity-quadratic"
                  className="pixel-input input-short"
                  type="number"
                  min={0}
                  max={1000}
                  value={"quadratic" in coeff ? coeff.quadratic : 0}
                  onChange={(e) =>
                    set("curve", { polynomial: { ...coeff, quadratic: Number(e.target.value) } } as Curve)
                  }
                />
              </Field>
            )}
          </div>
        )}
        <div className="form-grid">
          {number("message_min", "Minimum message XP", 1, 1000)}
          {number("message_max", "Maximum message XP", 1, 1000)}
          {number("message_cooldown_secs", "Message cooldown (seconds)", 5, 3600)}
          {number("reaction_xp", "Reaction XP", 1, 100)}
          {number("effort_bonus", "Effort bonus XP", 1, 100)}
          {number("vote_xp", "Vote XP", 1, 100)}
          {number("leave_grace_secs", "Leave grace (seconds)", 0, 604800)}
        </div>
        <div className="list">
          {toggle("messages", "Messages")}
          {toggle("reactions", "Reactions")}
          {toggle("voice", "Voice time")}
          {toggle("effort", "Long messages and attachments")}
          {toggle("votes", "Votes")}
          {toggle("separate_voice_level", "Separate voice level")}
          {toggle("reset_on_leave", "Reset XP when a member leaves")}
        </div>
        {!messageContent && (
          <p className="notice" role="status">
            {tr("Long-message effort needs the Message Content intent. Attachment bonuses still work.")}
          </p>
        )}
        <div className="form-grid">
          <Field label={tr("Highlights channel ID")} htmlFor="parity-highlights">
            <ChannelPicker
              id="parity-highlights"
              info={info}
              value={draft.highlights_channel_id}
              onChange={(id) => set("highlights_channel_id", id)}
            />
          </Field>
          <SwitchRow
            title={tr("Public leaderboard")}
            control={
              <PixelToggle
                label={tr("Public leaderboard")}
                checked={draft.vanity_slug !== null}
                onChange={(on) => {
                  if (!on) setLastSlug(draft.vanity_slug ?? "");
                  set("vanity_slug", on ? lastSlug || `server-${guild}` : null);
                }}
              />
            }
          >
            {tr(
              "Anyone with the link can view this board. Turning it off removes the public URL when you save.",
            )}
          </SwitchRow>
          <Field label={tr("Leaderboard URL name")} htmlFor="parity-vanity">
            <input
              id="parity-vanity"
              className="pixel-input"
              maxLength={32}
              value={draft.vanity_slug ?? ""}
              disabled={draft.vanity_slug === null}
              onChange={(e) => set("vanity_slug", e.target.value)}
            />
          </Field>
        </div>
        {saved.vanity_slug && (
          <p style={{ overflowWrap: "anywhere" }}>
            <a href={`/lb/${saved.vanity_slug}`}>{`${window.location.origin}/lb/${saved.vanity_slug}`}</a>
          </p>
        )}
        {invalid && (
          <p className="field-error" role="alert">
            {tr("Check XP ranges and the URL name.")}
          </p>
        )}
        {data.save.isError && (
          <p className="field-error" role="alert">
            {apiError(data.save.error.message)}
          </p>
        )}
        <div className="add-actions">
          <PixelButton
            disabled={invalid || JSON.stringify(draft) === JSON.stringify(saved) || data.save.isPending}
            onClick={() => data.save.mutate(draft)}
          >
            {tr("Save XP options")}
          </PixelButton>
          <PixelButton
            variant="ghost"
            disabled={JSON.stringify(draft) === JSON.stringify(saved)}
            onClick={() => setDraft(saved)}
          >
            {tr("Reset")}
          </PixelButton>
        </div>
      </Panel>
      <Panel
        title={tr("VOICE LEVEL ROLES")}
        aside={<span className="pixel-label muted">{data.voiceRoles.data?.items.length ?? 0}/25</span>}
      >
        <p className="muted panel-lead">
          {tr("When separate voice levels are on, members earn these roles from voice time.")}
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
          <p className="muted">{tr("No voice level roles yet.")}</p>
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
            {apiError(data.putVoiceRole.error.message)}
          </p>
        )}
      </Panel>
      <Panel title={tr("TOP GAINERS THIS WEEK")}>
        <p className="muted panel-lead">
          {tr("The current UTC week starts Monday. Members with the most new server XP appear here.")}
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
      </Panel>
      <Panel title={tr("IMPORT LEVELS FROM ANOTHER BOT")}>
        <p className="muted panel-lead">
          {tr(
            "Upload an Arcane, Lurkr or Amari style JSON or CSV export. Preview the rows before applying them. Files stay private to this session for ten minutes.",
          )}
        </p>
        <div className="form-grid">
          <Field label={tr("Export file")} htmlFor="parity-file">
            <input
              id="parity-file"
              type="file"
              accept=".json,.csv,application/json,text/csv"
              onChange={readFile}
            />
          </Field>
          <Field label={tr("How to combine XP")} htmlFor="parity-mode">
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
              <option value="merge">{tr("Keep higher XP")}</option>
              <option value="replace">{tr("Use imported XP")}</option>
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
          {tr("Preview import")}
        </PixelButton>
        {data.preview.isError && (
          <p role="alert" className="field-error">
            {apiError(data.preview.error.message)}
          </p>
        )}
        {previewId && data.preview.data && (
          <div>
            <p>
              {tr("Members in file")}: {data.preview.data.count} · {tr("Members changing")}:{" "}
              {data.preview.data.changed}
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
              {tr("Apply import")}
            </PixelButton>
          </div>
        )}
        {data.apply.isSuccess && (
          <p role="status">
            {tr("Import complete")}: {data.apply.data.updated}/{data.apply.data.checked}
          </p>
        )}
        {data.apply.isError && (
          <p role="alert" className="field-error">
            {apiError(data.apply.error.message)}
          </p>
        )}
      </Panel>
    </>
  );
}
