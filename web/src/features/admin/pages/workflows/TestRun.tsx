import { t } from "../../../../i18n/workflows";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { Player } from "./Player";
import { FakeEvent } from "./FakeEvent";
import type { Editor } from "../../hooks/useWorkflowEditor";
export function TestRun({ editor }: { editor: Editor }) {
  const { editing, test, fake, fakeVars, cooldowns } = editor;
  if (!editing) return null;
  return (
    <Panel title={t("Test run")}>
      <p>{t("Test with a fake event. No messages, roles, points, variables or history will change.")}</p>
      <FakeEvent editor={editor} />
      <PixelButton
        disabled={test.isPending}
        onClick={() =>
          test.mutate({
            definition: editing,
            event: {
              ...fake,
              kind: editing.trigger.kind,
              role_id: editing.trigger.role_id ?? "",
              variables: Object.fromEntries(
                fakeVars
                  .split("\n")
                  .filter((v) => v.includes("="))
                  .map((v) => [v.slice(0, v.indexOf("=")), v.slice(v.indexOf("=") + 1)]),
              ),
              cooldowns: cooldowns.split(",").filter(Boolean).map(Number),
            },
          })
        }
      >
        {t("Test run")}
      </PixelButton>
      {test.data && <Player key={test.submittedAt} playback={test.data.playback} />}
    </Panel>
  );
}
