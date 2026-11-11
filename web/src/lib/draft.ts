// Unsaved edits: what differs between the saved values and the draft.

/** The fields of `draft` whose value differs from `saved`. */
export function changedFields<T extends object>(saved: T, draft: Partial<T>): (keyof T)[] {
  return (Object.keys(draft) as (keyof T)[]).filter(
    (key) => draft[key] !== undefined && !Object.is(draft[key], saved[key]),
  );
}

/** Only the changed fields of `draft`, ready to PATCH. */
export function patchOf<T extends object>(saved: T, draft: Partial<T>): Partial<T> {
  const patch: Partial<T> = {};
  for (const key of changedFields(saved, draft)) patch[key] = draft[key];
  return patch;
}

/** Saved values for the exact fields in a patch, suitable for a second save call. */
export function undoPatch<T extends object>(saved: T, patch: Partial<T>): Partial<T> {
  const previous: Partial<T> = {};
  for (const key of Object.keys(patch) as (keyof T)[]) previous[key] = saved[key];
  return previous;
}

export function unsavedLabel(count: number): string {
  return count === 1 ? "1 unsaved change" : `${count} unsaved changes`;
}
