import { translate as spaTranslate } from "../../../i18n";
import { useCallback, useState } from "react";
import { Field } from "../../../components/Field";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dashboardT } from "../../../i18n/dashboard";
import { statsSummary } from "./statsSummary";
import { CounterList } from "./CounterList";
import { ChannelPicker, RolePicker } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { RichText } from "../../../components/RichText";
import { Toast } from "../../../components/Toast";
import { Loading, Problem } from "../../../components/States";
import { ApiError } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { type Counter, type CounterPreview, previewCounter, useCounters } from "../api/stats";

export function CounterChannels({ guild }: { guild: string }) {
  const discord = useDiscord(guild);
  const { query, add, remove } = useCounters(guild);
  const [channel, setChannel] = useState<string | null>(null);
  const [kind, setKind] = useState("members");
  const [role, setRole] = useState<string | null>(null);
  const [template, setTemplate] = useState("");
  const [preview, setPreview] = useState<CounterPreview | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const clearToast = useCallback(() => setToast(null), []);
  const list = query.data;
  const counters = list?.counters ?? [];
  const max = list?.max ?? 10;
  const chosen = list?.kinds.find((k) => k.key === kind);
  const existing = counters.find((c) => c.channel_id === channel);
  const full = counters.length >= max && !existing;
  const templateProblem =
    template.trim() && !template.includes("{count}")
      ? spaTranslate("Put {count} where the number goes.")
      : template.length > (list?.template_max ?? 80)
        ? spaTranslate("Keep it to {arg1} characters.", { arg1: list?.template_max ?? 80 })
        : null;
  const refused = add.error instanceof ApiError ? add.error : null;
  const fieldError = (field: string) => refused?.errors.find((e) => e.field === field)?.message ?? null;
  const draft = { kind, role_id: chosen?.needs_role ? role : null, template: template.trim() || null };

  const runPreview = () => {
    setPreviewing(true);
    previewCounter(guild, draft)
      .then(setPreview)
      .catch((error: unknown) =>
        setPreview({
          name: null,
          value: null,
          problem:
            error instanceof Error ? error.message : spaTranslate("The preview could not be worked out."),
        }),
      )
      .finally(() => setPreviewing(false));
  };

  const edit = (counter: Counter) => {
    setChannel(counter.channel_id);
    setKind(counter.kind);
    setRole(counter.role_id);
    setTemplate(counter.template);
    setPreview(null);
  };

  return (
    <div className="settings-detail-list">
      <SettingsRow
        title={spaTranslate("COUNTER CHANNELS")}
        description={dashboardT("Manage channels whose names show live counts.")}
        summary={statsSummary(list, false).counters}
        initialOpen
      >
        <div className="settings-detail-fields">
          <span className="pixel-label muted">
            {spaTranslate("{count}/{max} USED", { count: counters.length, max })}
          </span>
          <p className="muted panel-lead">
            {" "}
            {spaTranslate("A channel whose name shows a live number, like")}{" "}
            <strong>{spaTranslate("Members: 1,234")}</strong>
            {spaTranslate(
              ". Pinchora renames it when the number changes, at most once every 10 minutes. A locked voice channel works best. Same as",
            )}{" "}
            <code>/integrations stats</code> {spaTranslate("in Discord.")}{" "}
          </p>
          {query.isPending && <Loading label={spaTranslate("Reading the counters")} />}
          {query.isError && <Problem error={query.error} />}
          <CounterList
            list={list}
            counters={counters}
            info={discord.data}
            pending={remove.isPending}
            onEdit={edit}
            onRemove={(id) =>
              remove.mutate(id, {
                onSuccess: () => setToast(spaTranslate("Removed. The channel keeps its last name.")),
              })
            }
          />

          <div className={`add-row add-row-counter ${chosen?.needs_role ? "add-row-counter-3" : ""}`}>
            <Field label={spaTranslate("Channel")} htmlFor="counter-channel" error={fieldError("channel_id")}>
              <ChannelPicker
                id="counter-channel"
                info={discord.data}
                value={channel}
                onChange={(id) => {
                  setChannel(id);
                  const there = counters.find((c) => c.channel_id === id);
                  if (there) edit(there);
                }}
                any
                invalid={!!fieldError("channel_id")}
              />
            </Field>
            <Field label={spaTranslate("Counts")} htmlFor="counter-kind" error={fieldError("kind")}>
              <select
                id="counter-kind"
                className="pixel-input pixel-select"
                value={kind}
                onChange={(event) => {
                  setKind(event.target.value);
                  setPreview(null);
                }}
              >
                {(list?.kinds ?? []).map((k) => (
                  <option key={k.key} value={k.key}>
                    {spaTranslate(k.label)}
                  </option>
                ))}
              </select>
            </Field>
            {chosen?.needs_role && (
              <Field label={spaTranslate("Role")} htmlFor="counter-role" error={fieldError("role_id")}>
                <RolePicker id="counter-role" info={discord.data} value={role} onChange={setRole} />
              </Field>
            )}
            <div className="field-wide">
              <Field
                label={spaTranslate("Name")}
                htmlFor="counter-template"
                error={templateProblem ?? fieldError("template")}
                hint={
                  <>
                    {" "}
                    {spaTranslate("Leave it empty for")}{" "}
                    <strong>{chosen?.template ?? "Members: {count}"}</strong>
                    {spaTranslate(". Write")} <code>{"{count}"}</code>{" "}
                    {spaTranslate("where the number goes.")}{" "}
                  </>
                }
              >
                <input
                  id="counter-template"
                  className="pixel-input"
                  value={template}
                  maxLength={(list?.template_max ?? 80) + 10}
                  placeholder={chosen?.template}
                  onChange={(event) => {
                    setTemplate(event.target.value);
                    setPreview(null);
                  }}
                  aria-invalid={templateProblem ? true : undefined}
                  aria-describedby="counter-template-hint"
                />
              </Field>
            </div>
          </div>
          <div className="counter-preview">
            <PixelButton
              variant="secondary"
              disabled={!!templateProblem || previewing || (chosen?.needs_role && !role)}
              onClick={runPreview}
            >
              {previewing ? spaTranslate("Asking Discord…") : spaTranslate("Preview name")}
            </PixelButton>
            <PixelButton
              disabled={
                !channel || full || !!templateProblem || add.isPending || (chosen?.needs_role && !role)
              }
              onClick={() =>
                add.mutate(
                  { ...draft, channel_id: channel },
                  {
                    onSuccess: () => {
                      setToast(
                        existing
                          ? spaTranslate("Counter updated.")
                          : spaTranslate("Counter added. The channel is renamed in a moment."),
                      );
                      setChannel(null);
                      setTemplate("");
                      setPreview(null);
                    },
                  },
                )
              }
            >
              {existing ? spaTranslate("Update counter") : spaTranslate("Add counter")}
            </PixelButton>
            {preview && (
              <p role="status">
                {preview.name && (
                  <>
                    <span className="muted">{spaTranslate("It would read")} </span>
                    <span className="counter-preview-name">{preview.name}</span>
                  </>
                )}
                {preview.problem && (
                  <span className="muted">
                    {" "}
                    <RichText text={preview.problem} />
                  </span>
                )}
              </p>
            )}
            {full && (
              <p className="muted">
                {spaTranslate("All")} {max} {spaTranslate("counters are in use: remove one to add another.")}
              </p>
            )}
          </div>
          {refused && !refused.field && (
            <p className="field-error" role="alert">
              <RichText text={refused.message} />
            </p>
          )}
          <Toast message={toast} onDone={clearToast} />
        </div>
      </SettingsRow>
    </div>
  );
}
