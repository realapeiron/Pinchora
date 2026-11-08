import { t } from "../../i18n/workflows";
import { Panel } from "../../components/Panel";
import type { Data } from "../../lib/workflows";
import { NodeFields } from "./Steps";
import { PostButton } from "./PostButton";
import type { Editor } from "./useWorkflowEditor";
export function TriggerPanel({ editor, data, guild }: { editor: Editor; data: Data; guild: string }) {
  const { editing, setEditing, id, path } = editor;
  if (!editing) return null;
  return (
    <Panel title={t("Trigger")}>
      <label>
        {t("Trigger")}
        <select
          className="pixel-input pixel-select"
          value={editing.trigger.kind}
          onChange={(e) =>
            setEditing({
              ...editing,
              trigger: structuredClone(data.triggers.find((n) => n.kind === e.target.value)!),
              enabled:
                e.target.value === "message_keyword" && !data.message_content ? false : editing.enabled,
            })
          }
        >
          {!data.triggers.some((n) => n.kind === editing.trigger.kind) && (
            <option value={editing.trigger.kind} disabled>
              {t("Unavailable trigger")}
            </option>
          )}
          {data.triggers.map((n) => (
            <option key={n.kind} value={n.kind}>
              {t(n.kind)}
            </option>
          ))}
        </select>
      </label>
      <NodeFields
        node={editing.trigger}
        change={(trigger) => setEditing({ ...editing, trigger })}
        guild={guild}
      />
      {editing.trigger.kind === "command" && <code>/tag wf:{editing.name}</code>}
      {editing.trigger.kind === "button" && id && <PostButton path={`${path}/${id}/post`} />}
    </Panel>
  );
}
