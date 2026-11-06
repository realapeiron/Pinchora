// A command or responder being written: the draft over what was saved, the problems the server found, and
// saving it. Leaving with unsaved edits is the SaveBar's business.

import { useCallback, useState } from "react";
import { flushSync } from "react-dom";

import { ApiError, type FieldError } from "./api";
import { changedKeys } from "./tags";

export function useTagEditor<Def extends object>(
  initial: Def,
  /** What is sent: texts tidied, empty embeds left out. */
  tidy: (draft: Def) => Def,
  submit: (def: Def) => Promise<unknown>,
) {
  const [saved, setSaved] = useState(initial);
  const [draft, setDraft] = useState(initial);
  const [errors, setErrors] = useState<FieldError[]>([]);
  const [failure, setFailure] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const set = useCallback(<K extends keyof Def>(key: K, value: Def[K]) => {
    setDraft((d) => ({ ...d, [key]: value }));
    setErrors((list) => list.filter((e) => e.field !== key));
    setFailure(null);
  }, []);

  /** Several fields at once (a trigger change resets what depends on it). */
  const patch = useCallback((changes: Partial<Def>) => {
    setDraft((d) => ({ ...d, ...changes }));
    setErrors((list) => list.filter((e) => !(e.field in changes)));
    setFailure(null);
  }, []);

  const changes = changedKeys(saved, draft).length;
  const problem = (field: string) => errors.find((e) => e.field === field)?.message ?? null;

  /** Saves; `after` runs once the draft counts as saved (so leaving the page is not blocked). */
  const save = async (after: () => void) => {
    setSaving(true);
    setErrors([]);
    setFailure(null);
    try {
      await submit(tidy(draft));
      flushSync(() => setSaved(draft));
      after();
    } catch (error) {
      if (error instanceof ApiError) {
        setErrors(error.errors);
        // A refusal about the whole thing (a limit, the intent) has no field to sit at.
        if (error.errors.length === 0) setFailure(error.message);
        else if (error.errors.length > 1) setFailure(`${error.errors.length} things to fix, marked below`);
        else setFailure(error.message);
      } else {
        setFailure("Something went wrong. Please try again.");
      }
    } finally {
      setSaving(false);
    }
  };

  const reset = () => {
    setDraft(saved);
    setErrors([]);
    setFailure(null);
  };

  return { draft, set, patch, changes, saving, problem, failure, save, reset };
}
