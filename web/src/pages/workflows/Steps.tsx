import { translate as t } from "../../i18n";
import { PixelButton } from "../../components/PixelButton";
import { EmbedEditor } from "../../components/EmbedEditor";
import { PlaceholderChips } from "../../components/PlaceholderChips";
import type { EmbedSpec } from "../../lib/api";
import { move, conditions, actions, type Node } from "../../lib/workflows";
const labels: Record<string, string> = {
  role_id: "Role",
  channel_id: "Channel",
  level: "Level",
  days: "Account age in days",
  percent: "Chance (%)",
  seconds: "Cooldown (seconds)",
  scope: "Scope",
  key: "Key",
  value: "Value",
  amount: "Amount",
  start: "Start minute",
  end: "End minute",
  tz: "Timezone",
  hour: "Hour",
  minute: "Minute",
  weekday: "Weekday (Monday = 0)",
  minutes: "Interval (minutes)",
  text: "Message",
  name: "Thread name",
  panel_id: "Panel ID",
  topic: "Topic index",
  emoji: "Emoji",
  keyword: "Keyword",
};
export function NodeFields({
  node,
  change,
  guild,
}: {
  node: Node;
  change: (node: Node) => void;
  guild: string;
}) {
  if (node.kind === "tag")
    return (
      <NodeFields node={node.action as Node} change={(action) => change({ ...node, action })} guild={guild} />
    );
  if (node.kind === "random_branch") return <Branches node={node} change={change} guild={guild} />;
  return (
    <div className="wf-fields">
      {Object.entries(node)
        .filter(([key]) => key !== "kind" && key !== "embed" && key !== "spec")
        .map(([key, value]) => (
          <label key={key}>
            <span>{t(labels[key] ?? key)}</span>
            {key === "scope" ? (
              <select
                className="pixel-input pixel-select"
                value={String(value)}
                onChange={(e) => change({ ...node, [key]: e.target.value })}
              >
                <option value="server">{t("Server")}</option>
                <option value="member">{t("Member")}</option>
              </select>
            ) : key === "text" ? (
              <>
                <textarea
                  className="pixel-input"
                  aria-label={t("Message")}
                  maxLength={2000}
                  value={String(value)}
                  onChange={(e) => change({ ...node, text: e.target.value })}
                />
                <PlaceholderChips
                  placeholders={[
                    { token: "{user}", hint: "The triggering member" },
                    { token: "{channel}", hint: "The current channel" },
                  ]}
                  onPick={(token) => change({ ...node, text: `${String(value)}${token}` })}
                />
              </>
            ) : (
              <input
                className="pixel-input"
                type={typeof value === "number" ? "number" : "text"}
                value={String(value)}
                onChange={(e) =>
                  change({
                    ...node,
                    [key]: typeof value === "number" ? Number(e.target.value) : e.target.value,
                  })
                }
              />
            )}
          </label>
        ))}
      {node.kind === "schedule" && <ScheduleFields node={node} change={change} guild={guild} />}
      {"embed" in node && (
        <EmbedEditor
          embed={node.embed as EmbedSpec | null}
          onChange={(embed) => change({ ...node, embed })}
        />
      )}
    </div>
  );
}
export function Steps({
  items,
  change,
  kind,
  guild,
  nested = false,
}: {
  items: Node[];
  change: (items: Node[]) => void;
  kind: "condition" | "action";
  guild: string;
  nested?: boolean;
}) {
  const options = kind === "condition" ? conditions : actions;
  return (
    <div className="wf-steps">
      {items.map((item, i) => (
        <fieldset key={i} className="wf-step">
          <legend>
            {t(kind)} {i + 1}
          </legend>
          <label>
            {t(kind)}
            <select
              className="pixel-input pixel-select"
              value={item.kind === "tag" ? (item.action as Node).kind : item.kind}
              onChange={(e) =>
                change(items.map((n, j) => (i === j ? structuredClone(options[e.target.value]!) : n)))
              }
            >
              {Object.keys(options)
                .filter((key) => !nested || key !== "random_branch")
                .map((key) => (
                  <option key={key} value={key}>
                    {t(key)}
                  </option>
                ))}
            </select>
          </label>
          <NodeFields
            node={item}
            change={(node) => change(items.map((n, j) => (i === j ? node : n)))}
            guild={guild}
          />
          <div className="wf-controls">
            <PixelButton variant="ghost" disabled={i === 0} onClick={() => change(move(items, i, -1))}>
              {t("Move up")}
            </PixelButton>
            <PixelButton
              variant="ghost"
              disabled={i === items.length - 1}
              onClick={() => change(move(items, i, 1))}
            >
              {t("Move down")}
            </PixelButton>
            <PixelButton variant="ghost" onClick={() => change(items.filter((_, j) => j !== i))}>
              {t("Remove")}
            </PixelButton>
          </div>
        </fieldset>
      ))}
      <PixelButton
        variant="secondary"
        disabled={items.length >= (kind === "condition" ? 5 : 10)}
        onClick={() =>
          change([...items, structuredClone(kind === "condition" ? conditions.has_role! : actions.reply!)])
        }
      >
        {t(kind === "condition" ? "Add condition" : "Add action")}
      </PixelButton>
    </div>
  );
}

function Branches({ node, change, guild }: { node: Node; change: (node: Node) => void; guild: string }) {
  const branches = node.branches as Node[][];
  return (
    <div>
      {branches.map((branch, i) => (
        <fieldset key={i}>
          <legend>
            {t("Branch")} {i + 1}
          </legend>
          <Steps
            items={branch}
            change={(value) => change({ ...node, branches: branches.map((b, j) => (i === j ? value : b)) })}
            kind="action"
            guild={guild}
            nested
          />
          <PixelButton
            variant="ghost"
            disabled={branches.length <= 1}
            onClick={() => change({ ...node, branches: branches.filter((_, j) => j !== i) })}
          >
            {t("Remove branch")}
          </PixelButton>
        </fieldset>
      ))}
      <PixelButton
        variant="secondary"
        disabled={branches.length >= 5}
        onClick={() => change({ ...node, branches: [...branches, [{ kind: "log", text: "" }]] })}
      >
        {t("Add branch")}
      </PixelButton>
    </div>
  );
}

function ScheduleFields({
  node,
  change,
  guild,
}: {
  node: Node;
  change: (node: Node) => void;
  guild: string;
}) {
  return (
    <>
      <label>
        {t("Schedule type")}
        <select
          className="pixel-input pixel-select"
          value={(node.spec as Node).kind}
          onChange={(e) =>
            change({
              ...node,
              spec:
                e.target.value === "interval"
                  ? { kind: "interval", minutes: 60 }
                  : e.target.value === "weekly"
                    ? { kind: "weekly", weekday: 0, hour: 9, minute: 0 }
                    : { kind: "daily", hour: 9, minute: 0 },
            })
          }
        >
          {["Daily", "Weekly", "Interval"].map((v) => (
            <option key={v} value={v.toLowerCase()}>
              {t(v)}
            </option>
          ))}
        </select>
      </label>
      <NodeFields node={node.spec as Node} change={(spec) => change({ ...node, spec })} guild={guild} />
    </>
  );
}
