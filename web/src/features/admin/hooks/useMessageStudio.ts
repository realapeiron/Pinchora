import { useEffect, useMemo, useState } from "react";
import { t as deferredText } from "../../../i18n/dashboard";
import { api, type Overview } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { importKit, studioError, type Component, type Kit } from "../api/messageStudio";
import { empty, slug, starter, type Saved, type Revision, type Rendered } from "../pages/studio/kitTemplates";

export function useMessageStudio(guild: Overview["guild"]) {
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
    void refresh().catch(() => setError(deferredText("Could not load message kits.")));
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
          setError(studioError(cause, deferredText("Preview failed.")));
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
    setNotice(deferredText("Template loaded. Edit it and save as a kit."));
  };
  const save = async () => {
    const selected = key || slug(draft.name);
    if (!selected) {
      setError(deferredText("Name this kit first."));
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
      setNotice(deferredText("Kit saved."));
    } catch (cause) {
      setError(studioError(cause, deferredText("Save failed.")));
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
      setNotice(deferredText("Discord message sent. Its ID is {arg1}.", { arg1: result.message_id }));
    } catch (cause) {
      setError(studioError(cause, deferredText("Discord did not accept the message.")));
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
      setNotice(deferredText("Version restored."));
    } catch (cause) {
      setError(studioError(cause, deferredText("Restore failed.")));
    } finally {
      setBusy(false);
    }
  };
  const importJson = async (file: File) => {
    if (file.size > 32 * 1024) {
      setError(deferredText("JSON must be at most 32 KiB."));
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
      setNotice(deferredText("JSON imported. Review and save it."));
    } catch {
      setError(deferredText("Invalid message kit JSON."));
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
      setNotice(deferredText("Event message updated."));
    } catch (cause) {
      setError(studioError(cause, deferredText("Event message could not be updated.")));
    }
  };

  return {
    discord,
    kits,
    key,
    draft,
    preview,
    revisions,
    bindings,
    channel,
    setChannel,
    messageId,
    setMessageId,
    busy,
    notice,
    error,
    dirty,
    set,
    choose,
    fresh,
    useStarter,
    save,
    send,
    restore,
    importJson,
    exportJson,
    editComponent,
    bind,
    setVariant,
  };
}

export type StudioModel = ReturnType<typeof useMessageStudio>;
