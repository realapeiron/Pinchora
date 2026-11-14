import { type Dispatch, type SetStateAction } from "react";
import { type QueryClient } from "@tanstack/react-query";
import { t as deferredText } from "../../../../i18n/dashboard";
import { ApiError, api, apiUpload } from "../../../../lib/api";
import {
  type CardDesign,
  type CardKind,
  type History,
  type CardUpload,
  type CardView,
  type Cards,
  CARD,
  replace,
} from "../../../../lib/card";
import { cardsKey, useDiscord } from "../../../../lib/queries";
import { type Draft, type Side, KINDS, sideOf, same } from "./welcomeDesign";

interface WelcomeOperationsInput {
  client: QueryClient;
  guild: string;
  draft: Draft;
  saved: Draft;
  kind: CardKind;
  setSaving: Dispatch<SetStateAction<boolean>>;
  setErrors: Dispatch<SetStateAction<string[]>>;
  setSaved: Dispatch<SetStateAction<Draft>>;
  setHist: Dispatch<SetStateAction<History<Draft>>>;
  refreshRevisions: (selected: CardKind) => Promise<void>;
  setToast: Dispatch<SetStateAction<string | null>>;
  setUploading: Dispatch<SetStateAction<boolean>>;
  setDesign: (next: CardDesign, merge?: string) => void;
  design: CardDesign;
  testChannel: string | null;
  setTesting: Dispatch<SetStateAction<boolean>>;
  side: Side;
  discord: ReturnType<typeof useDiscord>;
}

export function createWelcomeOperations({
  client,
  guild,
  draft,
  saved,
  kind,
  setSaving,
  setErrors,
  setSaved,
  setHist,
  refreshRevisions,
  setToast,
  setUploading,
  setDesign,
  design,
  testChannel,
  setTesting,
  side,
  discord,
}: WelcomeOperationsInput) {
  const save = async () => {
    setSaving(true);
    setErrors([]);
    const kinds = KINDS.map((k) => k.kind).filter((k) => !same(draft[k], saved[k]));
    try {
      const views = await Promise.all(
        kinds.map((k) =>
          api<CardView>(`/api/guilds/${guild}/cards/${k}`, "PUT", {
            design: draft[k].custom ? draft[k].design : null,
            delivery: draft[k].delivery,
          }),
        ),
      );
      const next = { ...saved };
      for (const view of views) next[view.kind] = sideOf(view);
      setSaved(next);
      setHist((h) =>
        replace(h, { ...h.present, ...Object.fromEntries(views.map((v) => [v.kind, sideOf(v)])) }),
      );
      client.setQueryData<Cards>(cardsKey(guild), (old) =>
        old ? { ...old, ...Object.fromEntries(views.map((v) => [v.kind, v])) } : old,
      );
      await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
      await refreshRevisions(kind);
      setToast(deferredText("Saved. The next matching event uses this design."));
    } catch (error) {
      setErrors(
        error instanceof ApiError
          ? error.errors.map((e) => e.message).concat(error.errors.length ? [] : [error.message])
          : [deferredText("The design could not be saved.")],
      );
    } finally {
      setSaving(false);
    }
  };

  const upload = async (file: File) => {
    if (file.size > CARD.upload_max_bytes) {
      setErrors([
        deferredText("A picture is at most {arg1} MB.", { arg1: CARD.upload_max_bytes / 1024 / 1024 }),
      ]);
      return;
    }
    setUploading(true);
    try {
      const made = await apiUpload<CardUpload>(`/api/guilds/${guild}/cards/uploads`, file);
      client.setQueryData<Cards>(cardsKey(guild), (old) =>
        old ? { ...old, uploads: [made, ...old.uploads] } : old,
      );
      setDesign({ ...design, background: { type: "upload", id: made.id } });
    } catch (error) {
      setErrors([
        error instanceof ApiError ? error.message : deferredText("The picture could not be uploaded."),
      ]);
    } finally {
      setUploading(false);
    }
  };

  const sendTest = async (dm = false) => {
    if (!dm && !testChannel) return;
    setTesting(true);
    try {
      await api(`/api/guilds/${guild}/cards/${kind}/test`, "POST", {
        design,
        delivery: side.delivery,
        channel_id: testChannel,
        dm,
      });
      const name = discord.data?.channels.find((c) => c.id === testChannel)?.name;
      setToast(
        dm
          ? deferredText("Test sent to your DMs.")
          : deferredText("Test posted{arg1}.", { arg1: name ? ` in #${name}` : "" }),
      );
    } catch (error) {
      setErrors(error instanceof ApiError ? [error.message] : [deferredText("The test could not be sent.")]);
    } finally {
      setTesting(false);
    }
  };

  return { save, upload, sendTest };
}
