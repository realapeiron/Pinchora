import { useOutletContext } from "react-router";
import { translate as t } from "../i18n";
import { SettingsPage } from "./PageHead";
import { Loading, Problem } from "./States";
import { Panel } from "../components/Panel";
import { PixelButton } from "../components/PixelButton";
import type { Overview } from "../lib/api";
import { useWorkflows, blank, type Data } from "../lib/workflows";
import { Steps } from "./workflows/Steps";
import { Variables } from "./workflows/Variables";
import { useWorkflowEditor } from "./workflows/useWorkflowEditor";
import { Toolbar } from "./workflows/Toolbar";
import { TriggerPanel } from "./workflows/TriggerPanel";
import { TestRun } from "./workflows/TestRun";
import { RunHistory } from "./workflows/RunHistory";
import "../styles/workflows.css";
export function Workflows() {
  const overview = useOutletContext<Overview>();
  const guild = overview.guild.id;
  const { query } = useWorkflows(guild);
  if (query.isPending) return <Loading />;
  if (query.isError) return <Problem error={query.error} />;
  return <Workspace guild={guild} data={query.data} />;
}
function Workspace({ guild, data }: { guild: string; data: Data }) {
  const editor = useWorkflowEditor(guild, data);
  const { editing, setEditing, notice, save, remove, test, path, open } = editor;
  return (
    <SettingsPage
      title={t("Workflows")}
      lead={t("Build a trigger, check conditions, then run bounded actions.")}
      className="settings-page wf-page"
    >
      <p className="muted">
        {t("25 workflows · 5 conditions · 10 actions · 5 Discord writes · 100 runs/minute")}
      </p>
      {!data.message_content && (
        <p className="notice" role="status">
          {t(
            "Message Content is off. Keyword workflows cannot be enabled until the bot operator enables this intent.",
          )}
        </p>
      )}
      {!editing ? (
        <Panel>
          <PixelButton disabled={data.items.length >= data.max} onClick={() => open(blank())}>
            {t("New workflow")}
          </PixelButton>
          {data.items.length === 0 && <p>{t("No workflows yet. Create one to get started.")}</p>}
          <ul className="wf-list">
            {data.items.map((row) => (
              <li key={row.id}>
                <button type="button" onClick={() => open(row, row.id)}>
                  {row.name}
                </button>
                <span>
                  {t(row.enabled ? "Enabled" : "Disabled")} · {row.runs}
                </span>
                {row.last_error && <span role="status">{t("failed")}</span>}
              </li>
            ))}
          </ul>
        </Panel>
      ) : (
        <fieldset className="wf-edit" disabled={save.isPending}>
          <Toolbar editor={editor} />
          {(save.error || remove.error || test.error) && (
            <p role="alert">{(save.error || remove.error || test.error)?.message}</p>
          )}
          {notice && <p role="status">{notice}</p>}
          <div className="wf-columns">
            <TriggerPanel editor={editor} data={data} guild={guild} />
            <Panel title={t("Conditions")}>
              <Steps
                items={editing.conditions}
                kind="condition"
                guild={guild}
                change={(conditions) => setEditing({ ...editing, conditions })}
              />
            </Panel>
            <Panel title={t("Actions")}>
              <Steps
                items={editing.actions}
                kind="action"
                guild={guild}
                change={(actions) => setEditing({ ...editing, actions })}
              />
            </Panel>
          </div>
          <TestRun editor={editor} />
          <RunHistory editor={editor} />
        </fieldset>
      )}
      <Variables path={path} />
    </SettingsPage>
  );
}
