import { translate as spaTranslate } from "../i18n";
// One settings page's form: a draft over the saved settings, the problems with it (the browser's
// instant checks, then what the server said), and saving the changed fields as one PATCH.

import { useCallback, useState } from "react";

import { ApiError, type FieldError, type Settings } from "./api";
import { changedFields, patchOf, undoPatch } from "./draft";
import { useSaveSettings } from "./queries";

export function useSettingsForm(
  guild: string,
  saved: Settings,
  check: (value: Settings, changed: (keyof Settings)[]) => (FieldError | null)[],
) {
  const [draft, setDraft] = useState<Partial<Settings>>({});
  const [serverErrors, setServerErrors] = useState<FieldError[]>([]);
  const [toast, setToast] = useState<string | null>(null);
  const [previous, setPrevious] = useState<Partial<Settings> | null>(null);
  const save = useSaveSettings(guild);
  const value: Settings = { ...saved, ...draft };
  const changed = changedFields(saved, draft);
  const local = check(value, changed).filter((e): e is FieldError => e !== null);
  const errors: Record<string, string> = {};
  for (const e of [...serverErrors, ...local]) errors[e.field] = e.message;

  const set = useCallback(<K extends keyof Settings>(field: K, next: Settings[K]) => {
    setDraft((d) => ({ ...d, [field]: next }));
    setPrevious(null);
    setServerErrors((list) => list.filter((e) => e.field !== field));
  }, []);

  const submit = () => {
    if (local.length > 0) return;
    const patch = patchOf(saved, draft);
    const before = undoPatch(saved, patch);
    save.mutate(patch, {
      onSuccess: () => {
        setDraft({});
        setServerErrors([]);
        setPrevious(before);
        setToast("Saved. The bot uses it from the next message.");
      },
      onError: (error) => setServerErrors(error instanceof ApiError ? error.errors : []),
    });
  };

  const undo = () => {
    if (!previous) return;
    save.mutate(previous, {
      onSuccess: () => {
        setPrevious(null);
        setToast("Previous values restored.");
      },
      onError: (error) => setToast(error.message),
    });
  };

  const reset = () => {
    setDraft({});
    setServerErrors([]);
    save.reset();
  };

  const barError =
    local.length > 0
      ? local.length === 1
        ? "Fix the marked field first"
        : spaTranslate("Fix the {arg1} marked fields first", { arg1: local.length })
      : save.isError && !(save.error instanceof ApiError && save.error.errors.length > 0)
        ? save.error.message
        : null;

  return {
    value,
    set,
    errors,
    changes: changed.length,
    saving: save.isPending,
    blocked: local.length > 0,
    barError,
    submit,
    undo: previous ? undo : undefined,
    reset,
    toast,
    clearToast: useCallback(() => setToast(null), []),
  };
}

export type SettingsForm = ReturnType<typeof useSettingsForm>;
