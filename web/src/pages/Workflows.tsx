import { useState } from "react";
import { useOutletContext } from "react-router";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { translate as t, dateTime } from "../i18n";
import { SettingsPage } from "./PageHead";
import { Loading, Problem } from "./States";
import { Panel } from "../components/Panel";
import { PixelButton } from "../components/PixelButton";
import { EmbedEditor } from "../components/EmbedEditor";
import { PlaceholderChips } from "../components/PlaceholderChips";
import { IdList } from "../components/IdList";
import { api, type Overview, type EmbedSpec } from "../lib/api";
import { useDiscord } from "../lib/queries";
import {
  useWorkflows,
  blank,
  payload,
  move,
  conditions,
  actions,
  type Node,
  type Definition,
  type Data,
  type Playback,
  type Run,
} from "../lib/workflows";
import "../styles/workflows.css";
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
function NodeFields({ node, change, guild }: { node: Node; change: (node: Node) => void; guild: string }) {
  if (node.kind === "tag")
    return (
      <NodeFields node={node.action as Node} change={(action) => change({ ...node, action })} guild={guild} />
    );
  if (node.kind === "random_branch") {
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
      {node.kind === "schedule" && (
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
      )}
      {"embed" in node && (
        <EmbedEditor
          embed={node.embed as EmbedSpec | null}
          onChange={(embed) => change({ ...node, embed })}
        />
      )}
    </div>
  );
}
function Steps({
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
function Player({ playback }: { playback: Playback }) {
  const [index, setIndex] = useState(0);
  const step = playback.steps[index];
  return (
    <section className="wf-playback" aria-label={t("Playback")}>
      <p role="status">{t(playback.outcome)}</p>
      <ol>
        {playback.steps.map((s, i) => (
          <li key={i}>
            <button type="button" aria-current={i === index ? "step" : undefined} onClick={() => setIndex(i)}>
              {t(s.phase)} {s.index + 1} · {t(s.status)}
            </button>
          </li>
        ))}
      </ol>
      {step && <pre tabIndex={0}>{JSON.stringify(step.detail, null, 2)}</pre>}
    </section>
  );
}
export function Workflows() {
  const overview = useOutletContext<Overview>();
  const guild = overview.guild.id;
  const { query } = useWorkflows(guild);
  if (query.isPending) return <Loading />;
  if (query.isError) return <Problem error={query.error} />;
  return <Workspace guild={guild} data={query.data} />;
}
function Workspace({ guild, data }: { guild: string; data: Data }) {
  const [editing, setEditing] = useState<Definition | null>(null);
  const [id, setId] = useState<string | undefined>();
  const [saved, setSaved] = useState("");
  const [notice, setNotice] = useState("");
  const { save, remove, test, path } = useWorkflows(guild);
  const discord = useDiscord(guild);
  const [fake, setFake] = useState({
    user_id: "1",
    channel_id: "1",
    message_id: "1",
    value: "",
    roles: [] as string[],
    level: 1,
    account_age_days: 30,
    roll: 0,
  });
  const [fakeVars, setFakeVars] = useState("");
  const [cooldowns, setCooldowns] = useState("");
  const history = useQuery({
    queryKey: ["workflow-runs", guild, id],
    enabled: Boolean(id),
    queryFn: () => api<{ items: Run[] }>(`${path}/${id}/runs`),
  });
  const open = (def: Definition, nextId?: string) => {
    setEditing(payload(def));
    setId(nextId);
    setSaved(nextId ? JSON.stringify(payload(def)) : "");
    setNotice("");
    test.reset();
    save.reset();
  };
  const changed = editing && JSON.stringify(payload(editing)) !== saved;
  const locked = editing?.trigger.kind === "message_keyword" && !data.message_content;
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
        <Panel title={t("Workflows")}>
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
                      setSaved(JSON.stringify(payload(row)));
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
          {(save.error || remove.error || test.error) && (
            <p role="alert">{(save.error || remove.error || test.error)?.message}</p>
          )}
          {notice && <p role="status">{notice}</p>}
          <div className="wf-columns">
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
                        e.target.value === "message_keyword" && !data.message_content
                          ? false
                          : editing.enabled,
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
          <Panel title={t("Test run")}>
            <p>
              {t("Test with a fake event. No messages, roles, points, variables or history will change.")}
            </p>
            <details>
              <summary>{t("Fake event")}</summary>
              <div className="wf-fake">
                {(
                  [
                    "user_id",
                    "channel_id",
                    "message_id",
                    "value",
                    "level",
                    "account_age_days",
                    "roll",
                  ] as const
                ).map((key) => (
                  <label key={key}>
                    {t(
                      {
                        user_id: "User ID",
                        channel_id: "Channel",
                        message_id: "Message ID",
                        value: "Event value",
                        level: "Level",
                        account_age_days: "Account age in days",
                        roll: "Random seed",
                      }[key],
                    )}
                    <input
                      className="pixel-input"
                      value={fake[key]}
                      onChange={(e) =>
                        setFake({
                          ...fake,
                          [key]: typeof fake[key] === "number" ? Number(e.target.value) : e.target.value,
                        })
                      }
                    />
                  </label>
                ))}
              </div>
              <IdList
                kind="role"
                info={discord.data}
                ids={fake.roles}
                onChange={(roles) => setFake({ ...fake, roles })}
                max={25}
                name={t("Held roles")}
                none={t("No roles")}
              />
              <label>
                {t("Variable values (scope:key=value, one per line)")}
                <textarea
                  className="pixel-input"
                  value={fakeVars}
                  onChange={(e) => setFakeVars(e.target.value)}
                />
              </label>
              <label>
                {t("Failed cooldown indexes (comma separated)")}
                <input
                  className="pixel-input"
                  value={cooldowns}
                  onChange={(e) => setCooldowns(e.target.value)}
                />
              </label>
            </details>
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
          {id && (
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
          )}
        </fieldset>
      )}
      <Variables path={path} />
    </SettingsPage>
  );
}
function Variables({ path }: { path: string }) {
  const client = useQueryClient();
  const [key, setKey] = useState("");
  const [value, setValue] = useState("");
  const [scope, setScope] = useState("server");
  const [subject, setSubject] = useState("");
  const effectiveSubject = scope === "member" ? subject : "";
  const query = useQuery({
    queryKey: [path, "variables", effectiveSubject],
    queryFn: () =>
      api<{ items: [string, string, string][] }>(
        `${path}/variables?subject=${encodeURIComponent(effectiveSubject)}`,
      ),
  });
  const save = useMutation({
    mutationFn: () =>
      api(`${path}/variables`, "PUT", { scope, key, value, subject_id: effectiveSubject || null }),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: [path, "variables"] });
      setKey("");
      setValue("");
    },
  });
  return (
    <Panel title={t("Variables")}>
      <fieldset className="wf-fake wf-edit" disabled={save.isPending}>
        <label>
          {t("Scope")}
          <select
            className="pixel-input pixel-select"
            value={scope}
            onChange={(e) => setScope(e.target.value)}
          >
            <option value="server">{t("Server")}</option>
            <option value="member">{t("Member")}</option>
          </select>
        </label>
        {scope === "member" && (
          <label>
            {t("Subject ID (blank: you)")}
            <input className="pixel-input" value={subject} onChange={(e) => setSubject(e.target.value)} />
          </label>
        )}
        <label>
          {t("Variable key")}
          <input
            className="pixel-input"
            maxLength={32}
            value={key}
            onChange={(e) => setKey(e.target.value)}
          />
        </label>
        <label>
          {t("Variable value")}
          <input
            className="pixel-input"
            maxLength={200}
            value={value}
            onChange={(e) => setValue(e.target.value)}
          />
        </label>
        <PixelButton
          disabled={
            !key ||
            save.isPending ||
            query.data?.items.some(([s, k, v]) => s === scope && k === key && v === value)
          }
          onClick={() => save.mutate()}
        >
          {t("Save variable")}
        </PixelButton>
      </fieldset>
      {save.error && <p role="alert">{save.error.message}</p>}
      {save.isSuccess && <p role="status">{t("Variable saved.")}</p>}
      {query.isError && <Problem error={query.error} />}
      <dl>
        {query.data?.items.map(([s, k, v]) => (
          <div key={`${s}:${k}`}>
            <dt>
              <code>{`{var.${s}.${k}}`}</code>
            </dt>
            <dd>{v}</dd>
          </div>
        ))}
      </dl>
    </Panel>
  );
}

function PostButton({ path }: { path: string }) {
  const [channel, setChannel] = useState("");
  const [text, setText] = useState("");
  const [label, setLabel] = useState("");
  const post = useMutation({ mutationFn: () => api(path, "POST", { channel_id: channel, text, label }) });
  return (
    <fieldset className="wf-fields wf-edit" disabled={post.isPending}>
      <legend>{t("Post workflow button")}</legend>
      <label>
        {t("Channel")}
        <input className="pixel-input" value={channel} onChange={(e) => setChannel(e.target.value)} />
      </label>
      <label>
        {t("Button message")}
        <textarea
          className="pixel-input"
          maxLength={2000}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
      </label>
      <label>
        {t("Button label")}
        <input
          className="pixel-input"
          maxLength={80}
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
      </label>
      <PixelButton disabled={!channel || !text || !label} onClick={() => post.mutate()}>
        {t("Post workflow button")}
      </PixelButton>
      {post.error && <p role="alert">{post.error.message}</p>}
      {post.isSuccess && <p role="status">{t("Workflow button posted.")}</p>}
    </fieldset>
  );
}
