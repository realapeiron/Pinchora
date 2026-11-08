import { translate as t, dateTime } from "../../i18n";
import { Panel } from "../../components/Panel";
import { Loading, Problem } from "../States";
import { Player } from "./Player";
import type { Editor } from "./useWorkflowEditor";
export function RunHistory({ editor }: { editor: Editor }) {
  const { id, history } = editor;
  if (!id) return null;
  return (
    <Panel title={t("Run history")}>
      {history.isPending ? (
        <Loading />
      ) : history.isError ? (
        <Problem error={history.error} />
      ) : history.data.items.length === 0 ? (
        <p>{t("No runs in the last seven days.")}</p>
      ) : (
        history.data.items.map((run) => (
          <details key={run.id}>
            <summary>
              {dateTime(run.started_at)} · {t(run.outcome)}
            </summary>
            <Player playback={run} />
          </details>
        ))
      )}
    </Panel>
  );
}
