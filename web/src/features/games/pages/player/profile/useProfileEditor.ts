import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { t } from "../../../../../i18n";
import { api } from "../../../../../lib/api";
import {
  usePlayerPreview,
  profileChanges,
  reorderBadge,
  toggleBadge,
  type ProfileData,
  type ProfileDraft,
} from "../../../api/webPlayer";

type Tab = "badges" | "background" | "style" | "title" | "showcase" | "bio";
export function useProfileEditor(data: ProfileData) {
  const cache = useQueryClient();
  const [draft, setDraft] = useState(data.profile);
  const [saved, setSaved] = useState(data.profile);
  const [tab, setTab] = useState<Tab>("badges");
  const [notice, setNotice] = useState("");
  const [dragged, setDragged] = useState<number | null>(null);
  useEffect(() => {
    if (tab === "badges") void api("/api/me/badges/seen", "POST").catch(() => {});
  }, [tab]);
  const preview = usePlayerPreview(
    `/api/me/preview/profile-card.png?draft=${encodeURIComponent(JSON.stringify(draft))}`,
  );
  const update = <K extends keyof ProfileDraft>(key: K, value: ProfileDraft[K]) => {
    setDraft((p) => ({ ...p, [key]: value }));
    setNotice("");
  };
  const save = useMutation({
    mutationFn: (submitted: ProfileDraft) => api<ProfileDraft>("/api/me/profile", "PUT", submitted),
    onSuccess: (p, submitted) => {
      setSaved(p);
      setDraft((current) => (profileChanges(current, submitted) === 0 ? p : current));
      setNotice(t("Profile saved. Your next /profile will show this card."));
      void cache.invalidateQueries({ queryKey: ["player"] });
      void cache.invalidateQueries({ queryKey: ["player-preview"] });
    },
  });
  const buy = useMutation({
    mutationFn: ({ kind, id }: { kind: string; id: string }) =>
      api(`/api/me/${kind}/${encodeURIComponent(id)}/buy`, "POST"),
    onSuccess: () => {
      setNotice(t("Added to your collection."));
      void cache.invalidateQueries({ queryKey: ["player"] });
    },
  });
  const pick = (id: string) => {
    setDraft((p) => ({ ...p, badges: toggleBadge(p.badges, id), badges_set: true }));
    setNotice("");
  };
  const move = (from: number, to: number) => {
    setDraft((p) => ({ ...p, badges: reorderBadge(p.badges, from, to), badges_set: true }));
    setNotice(t("Badge moved to position {position}.", { position: to + 1 }));
  };
  const tabs: [Tab, string][] = [
    ["badges", t("Badges")],
    ["background", t("Background")],
    ["style", t("Style")],
    ["title", t("Title")],
    ["showcase", t("Showcase")],
    ["bio", t("Bio")],
  ];
  const unownedBackground =
    !!draft.background && !data.backgrounds.find((b) => b.id === draft.background)?.owned;
  return {
    data,
    draft,
    setDraft,
    saved,
    tab,
    setTab,
    notice,
    setNotice,
    dragged,
    setDragged,
    preview,
    update,
    save,
    buy,
    pick,
    move,
    tabs,
    unownedBackground,
  };
}

export type ProfileEditorModel = ReturnType<typeof useProfileEditor>;
