import { RolePicker, ChannelPicker } from "../../../components/Pickers";
import type { GuildInfo } from "../../../lib/api";
import type { AutoAction, Words } from "../api/automod";

export function ActionEditor({
  action: a,
  actions,
  info,
  t,
  onChange,
}: {
  action: AutoAction;
  actions: string[];
  info: GuildInfo | undefined;
  t: Words;
  onChange: (a: AutoAction) => void;
}) {
  return (
    <div className="automod-action">
      <select
        aria-label={t("ui.actions")}
        className="pixel-input pixel-select"
        value={a.kind}
        onChange={(e) =>
          onChange({ ...a, kind: e.target.value, secs: e.target.value === "timeout" ? 600 : 0 })
        }
      >
        {actions.map((k) => (
          <option key={k} value={k}>
            {t(`action.${k}`)}
          </option>
        ))}
      </select>
      {a.kind === "timeout" && (
        <input
          aria-label={t("ui.duration")}
          type="number"
          className="pixel-input"
          min="1"
          max="2419200"
          value={a.secs}
          onChange={(e) => onChange({ ...a, secs: Number(e.target.value) })}
        />
      )}{" "}
      {a.kind === "add_role" && (
        <RolePicker
          info={info}
          value={a.role}
          onChange={(role) => onChange({ ...a, role })}
          none={t("ui.pick")}
          assignable
          forActions
        />
      )}
    </div>
  );
}
export function IdList({
  title,
  values,
  roles,
  info,
  t,
  onChange,
}: {
  title: string;
  values: string[];
  roles: boolean;
  info: GuildInfo | undefined;
  t: Words;
  onChange: (values: string[]) => void;
}) {
  const add = (id: string | null) => {
    if (id && !values.includes(id)) onChange([...values, id]);
  };
  return (
    <div className="automod-id-list">
      <span className="field-label">
        {title}{" "}
        <span className="muted">
          ({values.length}/{roles ? 20 : 50})
        </span>
      </span>
      <div className="automod-chips">
        {values.map((id) => (
          <button
            type="button"
            className="channel-chip"
            key={id}
            aria-label={`${t("ui.remove")} ${id}`}
            onClick={() => onChange(values.filter((v) => v !== id))}
          >
            {roles ? "@" : "#"}
            {(roles ? info?.roles : info?.channels)?.find((r) => r.id === id)?.name ?? id} ×
          </button>
        ))}
      </div>
      {roles ? (
        <RolePicker
          info={info}
          value={null}
          onChange={add}
          none={t("ui.pick")}
          disabled={values.length >= 20}
        />
      ) : (
        <ChannelPicker
          label={title}
          info={info}
          value={null}
          onChange={add}
          none={t("ui.pick")}
          any
          disabled={values.length >= 50}
        />
      )}
    </div>
  );
}
