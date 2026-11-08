import { translate as t } from "../../i18n";
import { PixelButton } from "../../components/PixelButton";
import { payload, fingerprint } from "../../lib/workflows";
import type { Editor } from "./useWorkflowEditor";
export function Toolbar({ editor }: { editor: Editor }) {
  const { editing, setEditing, id, setId, setSaved, setNotice, save, remove, changed, locked } = editor;
  if (!editing) return null;
  return (
    <div className="wf-toolbar">
      <PixelButton variant="ghost" onClick={() => setEditing(null)}>
        {t("Back to workflows")}
      </PixelButton>
      <label>
        {t("Workflow name")}
        <input
          className="pixel-input"
          maxLength={40}
          value={editing.name}
          onChange={(e) => setEditing({ ...editing, name: e.target.value })}
        />
      </label>
      <label>
        <input
          className="wf-checkbox"
          type="checkbox"
          checked={editing.enabled}
          disabled={locked && !editing.enabled}
          onChange={(e) => setEditing({ ...editing, enabled: e.target.checked })}
        />
        {t("Enabled")}
      </label>
      <PixelButton
        disabled={!changed || save.isPending || !editing.name || Boolean(locked && editing.enabled)}
        onClick={() =>
          save.mutate(
            { id, def: editing },
            {
              onSuccess: (row) => {
                setId(row.id);
                setSaved(fingerprint(row));
                setEditing(payload(row));
                setNotice(t("Workflow saved."));
              },
            },
          )
        }
      >
        {t("Save workflow")}
      </PixelButton>
      {id && (
        <PixelButton
          variant="ghost"
          disabled={remove.isPending}
          onClick={() => remove.mutate(id, { onSuccess: () => setEditing(null) })}
        >
          {t("Delete workflow")}
        </PixelButton>
      )}
    </div>
  );
}
