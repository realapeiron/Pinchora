import { useState } from "react";
import { useOutletContext } from "react-router";
import type { Overview } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { words } from "../../moderation";
import {
  defaultRule,
  filteredCommands,
  useCommandControl,
  type ControlData,
  type ControlCommand,
} from "../api/commandControl";
import { PixelButton } from "../../../components/PixelButton";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dashboardT } from "../../../i18n/dashboard";
import { commandSummary, commandExtrasSummary } from "./commandSummary";
import { CommandDrawer } from "./CommandDrawer";
import { Field } from "../../../components/Field";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import "../../../styles/command-control.css";
export function CommandControl() {
  const { guild } = useOutletContext<Overview>();
  const { query } = useCommandControl(guild.id);
  if (query.isPending) return <Loading />;
  if (query.isError) return <Problem error={query.error} />;
  return <CommandEditor key={guild.id} guild={guild.id} data={query.data} />;
}
function CommandEditor({ guild, data }: { guild: string; data: ControlData }) {
  const w = words(data.strings);
  const { save } = useCommandControl(guild);
  const discord = useDiscord(guild);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<ControlCommand | null>(null);
  const [alias, setAlias] = useState("");
  const [target, setTarget] = useState("");
  const commands = filteredCommands(data.commands, search);
  const families = new Map<string, ControlCommand[]>();
  for (const command of commands) {
    const family = command.path.split(" ")[0] ?? command.path;
    families.set(family, [...(families.get(family) ?? []), command]);
  }
  const extras = commandExtrasSummary(data);
  return (
    <SettingsPage className="settings-page command-control-page" title={w("ui.title")} lead={w("ui.lead")}>
      <p className="muted">{w("ui.migration")}</p>
      <Field label={w("ui.search")} htmlFor="command-search">
        <input
          id="command-search"
          type="search"
          className="pixel-input"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </Field>
      {save.isError && (
        <p className="field-error" role="alert">
          {save.error.message}
        </p>
      )}
      {save.isSuccess && (
        <p className="notice" role="status">
          {w("saved")}
        </p>
      )}
      <div className="settings-detail-list">
        {commands.length === 0 && <p className="empty-row muted">{w("ui.empty")}</p>}
        {Array.from(families, ([family, familyCommands]) => (
          <SettingsRow
            key={family}
            title={`/${family}`}
            description={dashboardT("Edit the rules for this command family.")}
            summary={commandSummary(data, familyCommands)}
          >
            {/* eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- Scrollable command results need keyboard focus. */}
            <div className="command-control-list" role="region" aria-label={`/${family}`} tabIndex={0}>
              {familyCommands.map((command) => (
                <div key={command.path} className="command-row">
                  <button
                    type="button"
                    className="command-open"
                    onClick={() => setSelected(command)}
                    aria-label={`${w("ui.edit")}: /${command.path}`}
                  >
                    <strong>/{command.path}</strong>
                    <span className="muted">{command.description}</span>
                  </button>
                  {command.protected ? (
                    <span className="command-protected">{w("ui.protected")}</span>
                  ) : (
                    <label className="command-switch">
                      <input
                        type="checkbox"
                        checked={data.config.rules[command.path]?.enabled ?? true}
                        aria-label={`${w("ui.enabled")}: /${command.path}`}
                        disabled={save.isPending}
                        onChange={(e) =>
                          save.mutate({
                            ...data.config,
                            rules: {
                              ...data.config.rules,
                              [command.path]: {
                                ...(data.config.rules[command.path] ?? defaultRule()),
                                enabled: e.target.checked,
                              },
                            },
                          })
                        }
                      />
                      <span>{w("ui.enabled")}</span>
                    </label>
                  )}
                </div>
              ))}
            </div>
          </SettingsRow>
        ))}
        <SettingsRow
          title={w("ui.aliases")}
          description={dashboardT("Manage command aliases.")}
          summary={extras.aliases}
        >
          <p className="muted">{w("ui.alias_hint")}</p>
          <form
            className="command-alias-form"
            onSubmit={(e) => {
              e.preventDefault();
              save.mutate(
                { ...data.config, aliases: { ...data.config.aliases, [alias.trim().toLowerCase()]: target } },
                { onSuccess: () => setAlias("") },
              );
            }}
          >
            <Field label={w("ui.alias_name")} htmlFor="alias">
              <input
                id="alias"
                className="pixel-input"
                value={alias}
                maxLength={32}
                pattern="[a-zA-Z0-9_-]+"
                required
                onChange={(e) => setAlias(e.target.value)}
              />
            </Field>
            <Field label={w("ui.target")} htmlFor="alias-command">
              <select
                id="alias-command"
                className="pixel-input"
                required
                value={target}
                onChange={(e) => setTarget(e.target.value)}
              >
                <option value="">{w("ui.target")}</option>
                {data.commands.map((c) => (
                  <option key={c.path}>{c.path}</option>
                ))}
              </select>
            </Field>
            <PixelButton
              type="submit"
              disabled={save.isPending || Object.keys(data.config.aliases).length >= 50}
            >
              {w("ui.add")}
            </PixelButton>
          </form>
          <p className="muted">{Object.keys(data.config.aliases).length}/50</p>
          {Object.keys(data.config.aliases).length === 0 && <p className="muted">{w("ui.no_aliases")}</p>}
          {Object.entries(data.config.aliases).map(([name, path]) => (
            <div className="command-alias-row" key={name}>
              <code>
                {name} → {path}
              </code>
              <PixelButton
                variant="ghost"
                disabled={save.isPending}
                aria-label={`${w("ui.remove")}: ${name}`}
                onClick={() => {
                  const aliases = { ...data.config.aliases };
                  delete aliases[name];
                  save.mutate({ ...data.config, aliases });
                }}
              >
                {w("ui.remove")}
              </PixelButton>
            </div>
          ))}
        </SettingsRow>
        <SettingsRow
          title={w("ui.permissions")}
          description={dashboardT("Review Discord command permissions.")}
          summary={extras.permissions}
        >
          <p className="muted">{w("ui.permissions_hint")}</p>
          {data.permissions ? (
            <div className="command-permissions">
              {data.permissions.length === 0 && <p className="muted">{w("ui.no_permissions")}</p>}
              {data.permissions.map((entry) => (
                <section key={entry.id}>
                  <h3>{data.permission_names[entry.id] ?? w("ui.command_id", { id: entry.id })}</h3>
                  {entry.permissions.map((permission) => {
                    const kind =
                      permission.type === 1 ? "role" : permission.type === 3 ? "channel" : "member";
                    const name =
                      kind === "role"
                        ? discord.data?.roles.find((role) => role.id === permission.id)?.name
                        : kind === "channel"
                          ? discord.data?.channels.find((channel) => channel.id === permission.id)?.name
                          : undefined;
                    return (
                      <div className="command-alias-row" key={`${kind}:${permission.id}`}>
                        <span>
                          {w(`ui.${kind}`)}: {name ?? permission.id}
                        </span>
                        <strong>{w(permission.permission ? "ui.allow" : "ui.deny")}</strong>
                      </div>
                    );
                  })}
                </section>
              ))}
            </div>
          ) : (
            <p className="muted">{w("ui.unavailable")}</p>
          )}
        </SettingsRow>
      </div>
      {selected && (
        <CommandDrawer
          key={selected.path}
          command={selected}
          initial={data.config.rules[selected.path] ?? defaultRule()}
          info={discord.data}
          t={w}
          pending={save.isPending}
          error={save.error?.message}
          onClose={() => setSelected(null)}
          onSave={(rule) =>
            save.mutate(
              { ...data.config, rules: { ...data.config.rules, [selected.path]: rule } },
              { onSuccess: () => setSelected(null) },
            )
          }
        />
      )}
    </SettingsPage>
  );
}
