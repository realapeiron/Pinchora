import { useEffect, useMemo, useState } from "react";
import { useOutletContext } from "react-router";
import { translate as t } from "../../../i18n";
import { EmbedEditor } from "../../../components/EmbedEditor";
import { Field } from "../../../components/Field";
import { MessagePreview } from "../../../components/MessagePreview";
import { Panel } from "../../../components/Panel";
import { ChannelPicker } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { api, type EmbedSpec, type Overview } from "../../../lib/api";
import { blankEmbed } from "../../../lib/tags";
import { useDiscord } from "../../../lib/queries";
import { SettingsPage } from "../../shell/pages/PageHead";
import { importKit, componentRows, studioError, type Component, type Kit } from "../api/messageStudio";
import "../../../styles/message-studio.css";

interface Saved {
  key: string;
  name: string;
  payload: Kit;
  version: number;
}
interface Revision {
  version: number;
  created_at: string;
  payload: Kit;
}
interface Rendered {
  content: string;
  embeds: EmbedSpec[];
  components: Component[];
  allowed_mentions: string;
}
const empty = (): Kit => ({
  name: "",
  content: "",
  variants: [],
  embeds: [],
  components: [],
  allow_mass_mentions: false,
});
const slug = (name: string) =>
  name
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40);
const starter = (kind: "welcome" | "announcement" | "faq"): Kit => {
  if (kind === "welcome") {
    return {
      ...empty(),
      name: t("Welcome kit"),
      content: t("{{if returning}}Welcome back, {user}!{{else}}Welcome, {user}!{{end}}"),
      variants: [
        t("{{if returning}}Welcome back, {user}!{{else}}Welcome, {user}!{{end}}"),
        t("{{if returning}}Glad to see you again, {user}!{{else}}Glad you're here, {user}!{{end}}"),
      ],
    };
  }
  if (kind === "announcement") {
    return {
      ...empty(),
      name: t("Announcement kit"),
      content: t("News from {server}"),
      embeds: [
        { ...blankEmbed(), title: t("Announcement title"), description: t("Write your announcement here.") },
      ],
    };
  }
  return {
    ...empty(),
    name: t("FAQ kit"),
    content: t("Need help? Use the button below."),
    components: [
      { type: "button", label: t("Get help"), url: null, response: t("Ask a moderator for help.") },
    ],
  };
};

export function MessageStudio() {
  const { guild } = useOutletContext<Overview>();
  return <StudioEditor key={guild.id} guild={guild} />;
}

function StudioEditor({ guild }: { guild: Overview["guild"] }) {
  const discord = useDiscord(guild.id);
  const [kits, setKits] = useState<Saved[]>([]);
  const [key, setKey] = useState("");
  const [draft, setDraft] = useState<Kit>(empty);
  const [saved, setSaved] = useState<Kit | null>(null);
  const [preview, setPreview] = useState<Rendered | null>(null);
  const [revisions, setRevisions] = useState<Revision[]>([]);
  const [bindings, setBindings] = useState<Record<string, string>>({});
  const [channel, setChannel] = useState<string | null>(null);
  const [messageId, setMessageId] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [variant, setVariant] = useState(0);
  const path = `/api/guilds/${guild.id}/message-studio`;
  const dirty = JSON.stringify(draft) !== JSON.stringify(saved ?? empty());
  const payload = useMemo(() => JSON.stringify(draft), [draft]);
  const set = <K extends keyof Kit>(field: K, value: Kit[K]) => {
    setDraft((old) => ({ ...old, [field]: value }));
    setNotice("");
  };
  const refresh = async () => {
    setKits(await api<Saved[]>(path));
    setBindings(await api<Record<string, string>>(`${path}/events`));
  };
  const refreshHistory = async (selected: string) =>
    setRevisions(await api<Revision[]>(`${path}/${selected}/history`));
  useEffect(() => {
    void refresh().catch(() => setError(t("Could not load message kits.")));
  }, [path]);
  useEffect(() => {
    let active = true;
    setPreview(null);
    const timer = window.setTimeout(() => {
      if (!draft.content && draft.variants.length === 0 && draft.embeds.length === 0) {
        setPreview(null);
        return;
      }
      void api<Rendered>(`${path}/preview`, "POST", { payload: draft, variant })
        .then((data) => {
          if (!active) return;
          setPreview(data);
          setError("");
        })
        .catch((cause) => {
          if (!active) return;
          setPreview(null);
          setError(studioError(cause, t("Preview failed.")));
        });
    }, 400);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [path, payload, variant]);

  const choose = (item: Saved) => {
    setKey(item.key);
    setDraft(item.payload);
    setSaved(item.payload);
    setNotice("");
    void refreshHistory(item.key);
  };
  const fresh = () => {
    setKey("");
    setDraft(empty());
    setSaved(null);
    setRevisions([]);
    setMessageId("");
  };
  const useStarter = (kind: "welcome" | "announcement" | "faq") => {
    fresh();
    setDraft(starter(kind));
    setNotice(t("Template loaded. Edit it and save as a kit."));
  };
  const save = async () => {
    const selected = key || slug(draft.name);
    if (!selected) {
      setError(t("Name this kit first."));
      return;
    }
    setBusy(true);
    setError("");
    try {
      const item = await api<Saved>(`${path}/${selected}`, "PUT", draft);
      setKey(item.key);
      setDraft(item.payload);
      setSaved(item.payload);
      await refresh();
      await refreshHistory(item.key);
      setNotice(t("Kit saved."));
    } catch (cause) {
      setError(studioError(cause, t("Save failed.")));
    } finally {
      setBusy(false);
    }
  };
  const send = async () => {
    if (!channel || !key || dirty) return;
    setBusy(true);
    setError("");
    try {
      const result = await api<{ message_id: string }>(`${path}/send`, "POST", {
        key,
        channel_id: channel,
        message_id: messageId || null,
        variant,
      });
      setMessageId(result.message_id);
      setNotice(t("Discord message sent. Its ID is {arg1}.", { arg1: result.message_id }));
    } catch (cause) {
      setError(studioError(cause, t("Discord did not accept the message.")));
    } finally {
      setBusy(false);
    }
  };
  const restore = async (revision: Revision) => {
    setBusy(true);
    setError("");
    try {
      const item = await api<Saved>(`${path}/${key}/rollback/${revision.version}`, "POST");
      setDraft(item.payload);
      setSaved(item.payload);
      await refresh();
      await refreshHistory(key);
      setNotice(t("Version restored."));
    } catch (cause) {
      setError(studioError(cause, t("Restore failed.")));
    } finally {
      setBusy(false);
    }
  };
  const importJson = async (file: File) => {
    if (file.size > 32 * 1024) {
      setError(t("JSON must be at most 32 KiB."));
      return;
    }
    try {
      const imported = await importKit(await file.text(), (value) =>
        api<Kit>(`${path}/validate`, "POST", value),
      );
      setDraft(imported);
      setRevisions([]);
      setMessageId("");
      setKey("");
      setSaved(null);
      setNotice(t("JSON imported. Review and save it."));
    } catch {
      setError(t("Invalid message kit JSON."));
    }
  };
  const exportJson = () => {
    const url = URL.createObjectURL(new Blob([JSON.stringify(draft, null, 2)], { type: "application/json" }));
    const link = document.createElement("a");
    link.href = url;
    link.download = `${key || slug(draft.name) || "message-kit"}.json`;
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  };
  const editComponent = (index: number, next: Component) =>
    set(
      "components",
      draft.components.map((c, i) => (i === index ? next : c)),
    );
  const bind = async (kind: string, selected: string) => {
    try {
      await api(`${path}/events/${kind}`, "PUT", { key: selected || null });
      setBindings((old) => ({ ...old, [kind]: selected }));
      setNotice(t("Event message updated."));
    } catch (cause) {
      setError(studioError(cause, t("Event message could not be updated.")));
    }
  };

  return (
    <SettingsPage
      className="settings-page message-studio"
      title={t("MESSAGE STUDIO")}
      lead={t("Build, preview, save, and post Discord messages. Saved kits keep 20 versions.")}
    >
      <Panel title={t("SAVED KITS")} aside={<span className="counter">{kits.length}/50</span>}>
        <div className="studio-actions">
          <PixelButton variant="secondary" onClick={fresh}>
            {t("New kit")}
          </PixelButton>
          <select
            className="pixel-input pixel-select"
            aria-label={t("Saved kit")}
            value={key}
            onChange={(event) => {
              const item = kits.find((kit) => kit.key === event.target.value);
              if (item) choose(item);
              else fresh();
            }}
          >
            <option value="">{t("Choose a saved kit")}</option>
            {kits.map((kit) => (
              <option key={kit.key} value={kit.key}>
                {kit.name}
              </option>
            ))}
          </select>
          <label className="button pixel-button">
            {t("Import JSON")}
            <input
              type="file"
              accept="application/json,.json"
              onChange={(event) => {
                const file = event.target.files?.[0];
                if (file) void importJson(file);
                event.target.value = "";
              }}
            />
          </label>
          <PixelButton variant="ghost" onClick={exportJson}>
            {t("Export JSON")}
          </PixelButton>
        </div>
      </Panel>
      <Panel title={t("STARTER TEMPLATES")}>
        <p className="muted">{t("Choose a starting point, then customize and save it as a kit.")}</p>
        <div className="studio-actions">
          <PixelButton variant="secondary" onClick={() => useStarter("welcome")}>
            {t("Welcome kit")}
          </PixelButton>
          <PixelButton variant="secondary" onClick={() => useStarter("announcement")}>
            {t("Announcement kit")}
          </PixelButton>
          <PixelButton variant="secondary" onClick={() => useStarter("faq")}>
            {t("FAQ kit")}
          </PixelButton>
        </div>
      </Panel>
      <Panel title={t("MESSAGE")}>
        <div className="form-grid form-grid-2">
          <Field label={t("Kit name")} htmlFor="kit-name">
            <input
              id="kit-name"
              className="pixel-input"
              maxLength={60}
              value={draft.name}
              onChange={(event) => set("name", event.target.value)}
            />
          </Field>
          <Field label={t("Template key")} htmlFor="kit-key">
            <input id="kit-key" className="pixel-input" value={key || slug(draft.name)} readOnly />
          </Field>
        </div>
        <Field
          label={t("Content")}
          htmlFor="kit-content"
          hint={t(
            "Use {user}, {server}, {level}, {ordinal}, and if/else for booster, returning, account_age_days>=7, or ordinal>=100.",
          )}
        >
          <textarea
            id="kit-content"
            className="pixel-input"
            rows={5}
            maxLength={2000}
            value={draft.content}
            onChange={(event) => set("content", event.target.value)}
          />
          <span className="counter">{draft.content.length}/2000</span>
          <span className="muted">{t("Example: {{if returning}}Welcome back{{else}}Welcome{{end}}")}</span>
        </Field>
        <label className="studio-check">
          <input
            type="checkbox"
            checked={draft.allow_mass_mentions}
            onChange={(event) => set("allow_mass_mentions", event.target.checked)}
          />{" "}
          {t("Admin allows @everyone and role pings from this kit")}
        </label>
        <p className="muted">{t("Pings are blocked unless an admin explicitly enables them here.")}</p>
      </Panel>
      <Panel
        title={t("RANDOM MESSAGE GROUP")}
        aside={<span className="counter">{draft.variants.length}/10</span>}
      >
        <p className="muted">{t("When variants exist, one is chosen at random for each post.")}</p>
        {draft.variants.map((text, index) => (
          <div className="studio-row" key={index}>
            <textarea
              className="pixel-input"
              aria-label={t("Variant {arg1}", { arg1: index + 1 })}
              maxLength={2000}
              value={text}
              onChange={(event) =>
                set(
                  "variants",
                  draft.variants.map((v, i) => (i === index ? event.target.value : v)),
                )
              }
            />
            <PixelButton
              variant="ghost"
              onClick={() =>
                set(
                  "variants",
                  draft.variants.filter((_, i) => i !== index),
                )
              }
            >
              {t("Remove")}
            </PixelButton>
          </div>
        ))}
        <PixelButton
          variant="secondary"
          disabled={draft.variants.length >= 10}
          onClick={() => set("variants", [...draft.variants, draft.content || t("Hello {user}!")])}
        >
          {t("Add variant")}
        </PixelButton>
      </Panel>
      <Panel title={t("EMBEDS")} aside={<span className="counter">{draft.embeds.length}/10</span>}>
        {draft.embeds.map((embed, index) => (
          <div className="studio-embed" key={index}>
            <h3>{t("Embed {arg1}", { arg1: index + 1 })}</h3>
            <EmbedEditor
              embed={embed}
              onChange={(next) =>
                set(
                  "embeds",
                  next
                    ? draft.embeds.map((e, i) => (i === index ? next : e))
                    : draft.embeds.filter((_, i) => i !== index),
                )
              }
            />
          </div>
        ))}
        <PixelButton
          variant="secondary"
          disabled={draft.embeds.length >= 10}
          onClick={() => set("embeds", [...draft.embeds, blankEmbed()])}
        >
          {t("Add an embed")}
        </PixelButton>
      </Panel>
      <Panel
        title={t("BUTTONS AND SELECTS")}
        aside={<span className="counter">{componentRows(draft.components).length}/5</span>}
      >
        {draft.components.map((component, index) => (
          <div className="studio-component" key={index}>
            <div className="studio-actions">
              <strong>{component.type === "button" ? t("Button") : t("Select menu")}</strong>
              <PixelButton
                variant="ghost"
                onClick={() =>
                  set(
                    "components",
                    draft.components.filter((_, i) => i !== index),
                  )
                }
              >
                {t("Remove")}
              </PixelButton>
            </div>
            {component.type === "button" ? (
              <div className="form-grid form-grid-2">
                <Field label={t("Label")} htmlFor={`button-label-${index}`}>
                  <input
                    id={`button-label-${index}`}
                    className="pixel-input"
                    maxLength={80}
                    value={component.label}
                    onChange={(event) => editComponent(index, { ...component, label: event.target.value })}
                  />
                </Field>
                <Field label={t("HTTPS URL or leave blank for a reply")} htmlFor={`button-url-${index}`}>
                  <input
                    id={`button-url-${index}`}
                    className="pixel-input"
                    value={component.url ?? ""}
                    onChange={(event) =>
                      editComponent(index, {
                        ...component,
                        url: event.target.value || null,
                        response: event.target.value ? null : (component.response ?? t("Reply")),
                      })
                    }
                  />
                </Field>
                {!component.url && (
                  <Field label={t("Private reply")} htmlFor={`button-reply-${index}`}>
                    <input
                      id={`button-reply-${index}`}
                      className="pixel-input"
                      maxLength={2000}
                      value={component.response ?? ""}
                      onChange={(event) =>
                        editComponent(index, { ...component, response: event.target.value })
                      }
                    />
                  </Field>
                )}
              </div>
            ) : (
              <div>
                <Field label={t("Placeholder")} htmlFor={`select-placeholder-${index}`}>
                  <input
                    id={`select-placeholder-${index}`}
                    className="pixel-input"
                    maxLength={150}
                    value={component.placeholder}
                    onChange={(event) =>
                      editComponent(index, { ...component, placeholder: event.target.value })
                    }
                  />
                </Field>
                {component.options.map((option, optionIndex) => (
                  <div className="studio-row" key={optionIndex}>
                    <input
                      className="pixel-input"
                      aria-label={t("Option label")}
                      maxLength={100}
                      value={option.label}
                      onChange={(event) =>
                        editComponent(index, {
                          ...component,
                          options: component.options.map((o, i) =>
                            i === optionIndex ? { ...o, label: event.target.value } : o,
                          ),
                        })
                      }
                    />
                    <input
                      className="pixel-input"
                      aria-label={t("Option value")}
                      maxLength={100}
                      value={option.value}
                      onChange={(event) =>
                        editComponent(index, {
                          ...component,
                          options: component.options.map((o, i) =>
                            i === optionIndex ? { ...o, value: event.target.value } : o,
                          ),
                        })
                      }
                    />
                    <input
                      className="pixel-input"
                      aria-label={t("Private reply")}
                      maxLength={2000}
                      value={option.response}
                      onChange={(event) =>
                        editComponent(index, {
                          ...component,
                          options: component.options.map((o, i) =>
                            i === optionIndex ? { ...o, response: event.target.value } : o,
                          ),
                        })
                      }
                    />
                    <PixelButton
                      variant="ghost"
                      onClick={() =>
                        editComponent(index, {
                          ...component,
                          options: component.options.filter((_, i) => i !== optionIndex),
                        })
                      }
                    >
                      {t("Remove")}
                    </PixelButton>
                  </div>
                ))}
                <PixelButton
                  variant="secondary"
                  disabled={component.options.length >= 25}
                  onClick={() =>
                    editComponent(index, {
                      ...component,
                      options: [
                        ...component.options,
                        {
                          label: t("Choice"),
                          value: `choice_${component.options.length + 1}`,
                          response: t("Selected!"),
                        },
                      ],
                    })
                  }
                >
                  {t("Add choice")}
                </PixelButton>
              </div>
            )}
          </div>
        ))}
        <div className="studio-actions">
          <PixelButton
            variant="secondary"
            disabled={componentRows([...draft.components, { type: "button" }]).length > 5}
            onClick={() =>
              set("components", [
                ...draft.components,
                { type: "button", label: t("Button"), url: null, response: t("Reply") },
              ])
            }
          >
            {t("Add button")}
          </PixelButton>
          <PixelButton
            variant="secondary"
            disabled={componentRows([...draft.components, { type: "select" }]).length > 5}
            onClick={() =>
              set("components", [
                ...draft.components,
                {
                  type: "select",
                  placeholder: t("Choose"),
                  options: [{ label: t("Choice"), value: "choice_1", response: t("Selected!") }],
                },
              ])
            }
          >
            {t("Add select")}
          </PixelButton>
        </div>
      </Panel>
      <Panel title={t("LIVE DISCORD PREVIEW")}>
        {preview ? (
          <>
            <MessagePreview
              preview={{
                text: preview.content,
                embed: null,
                embeds: preview.embeds,
                components: preview.components,
                ephemeral: false,
                notes: [],
              }}
            />
            <p className="muted">
              {t("Allowed mentions: {arg1}", {
                arg1: preview.allowed_mentions === "none" ? t("No pings") : t("Everyone and roles"),
              })}
            </p>
            {draft.variants.length > 0 && (
              <div className="studio-actions">
                <PixelButton variant="ghost" onClick={() => setVariant((v) => v + 1)}>
                  {t("Preview another variant")}
                </PixelButton>
              </div>
            )}
          </>
        ) : (
          <p className="muted">{t("Add content or an embed to preview.")}</p>
        )}
      </Panel>
      <Panel title={t("POST OR EDIT IN DISCORD")}>
        <div className="studio-actions">
          <ChannelPicker label={t("Channel")} info={discord.data} value={channel} onChange={setChannel} />
          <input
            className="pixel-input"
            aria-label={t("Existing message ID to edit (optional)")}
            value={messageId}
            onChange={(event) => setMessageId(event.target.value)}
            placeholder={t("Message ID to edit (optional)")}
          />
          <PixelButton disabled={!channel || !key || dirty || busy || !preview} onClick={() => void send()}>
            {messageId ? t("Update message") : t("Send message")}
          </PixelButton>
        </div>
        <p className="muted">{t("Save the kit before posting. Enter a Pinchora message ID to edit it.")}</p>
      </Panel>
      <Panel title={t("AUTOMATIC EVENT MESSAGES")}>
        <p className="muted">
          {t(
            "Assign a text-only kit to welcome, goodbye, boost, or level up. Random variants and member conditions are evaluated when the event happens.",
          )}
        </p>
        <div className="form-grid form-grid-2">
          {(["welcome", "goodbye", "boost", "level_up"] as const).map((kind) => (
            <Field
              key={kind}
              label={t(
                kind === "level_up"
                  ? "Level up"
                  : kind === "boost"
                    ? "Boost"
                    : kind === "goodbye"
                      ? "Goodbye"
                      : "Welcome",
              )}
              htmlFor={`event-${kind}`}
            >
              <select
                id={`event-${kind}`}
                className="pixel-input pixel-select"
                value={bindings[kind] ?? ""}
                onChange={(event) => void bind(kind, event.target.value)}
              >
                <option value="">{t("Default message")}</option>
                {kits
                  .filter((item) => item.payload.embeds.length === 0 && item.payload.components.length === 0)
                  .map((item) => (
                    <option key={item.key} value={item.key}>
                      {item.name}
                    </option>
                  ))}
              </select>
            </Field>
          ))}
        </div>
      </Panel>
      <Panel title={t("SAVED VERSIONS")}>
        <div className="studio-actions">
          {revisions.map((revision) => (
            <div className="studio-revision" key={revision.version}>
              <span>
                v{revision.version} · {new Date(revision.created_at).toLocaleString()}
              </span>
              <PixelButton variant="secondary" disabled={busy} onClick={() => void restore(revision)}>
                {t("Restore")}
              </PixelButton>
            </div>
          ))}
          {revisions.length === 0 && <p className="muted">{t("No saved versions yet.")}</p>}
        </div>
      </Panel>
      <div className="studio-save">
        <PixelButton disabled={!dirty || busy} onClick={() => void save()}>
          {busy ? t("Working…") : t("Save kit")}
        </PixelButton>
        {error && (
          <p role="alert" className="field-error">
            {error}
          </p>
        )}
        {notice && <p role="status">{notice}</p>}
      </div>
    </SettingsPage>
  );
}
