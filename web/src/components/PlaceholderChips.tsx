import { translate as spaTranslate } from "../i18n";
import type { Placeholder } from "../lib/tags";

/** Buttons that put a placeholder into the text being written; hovering says what it becomes. */
export function PlaceholderChips({
  placeholders,
  onPick,
  label = spaTranslate("Insert a placeholder"),
}: {
  placeholders: Placeholder[];
  onPick: (token: string) => void;
  label?: string;
}) {
  return (
    <div className="chips" role="group" aria-label={label}>
      {placeholders.map((p) => (
        <button
          key={p.token}
          type="button"
          className="chip"
          title={spaTranslate(p.hint)}
          onClick={() => onPick(p.token)}
        >
          {p.token}
        </button>
      ))}
    </div>
  );
}
