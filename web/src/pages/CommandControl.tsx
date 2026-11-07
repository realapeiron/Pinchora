import { useEffect, useRef, useState } from "react";
import { useOutletContext } from "react-router";
import type { Overview, GuildInfo } from "../lib/api";
import { useDiscord } from "../lib/queries";
import { words, type Words } from "../lib/automod";
import {
  defaultRule,
  filteredCommands,
  useCommandControl,
  type ControlData,
  type ControlCommand,
  type CommandRule,
} from "../lib/commandControl";
import { PixelButton } from "../components/PixelButton";
import { Panel } from "../components/Panel";
import { Field } from "../components/Field";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";
import "../styles/command-control.css";
export function CommandControl() {
  const { guild } = useOutletContext<Overview>();
  const { query } = useCommandControl(guild.id);
  if (query.isPending) return <Loading />;
  if (query.isError) return <Problem error={query.error} />;
  return <CommandEditor guild={guild.id} data={query.data} />;
}
function CommandEditor({ guild, data }: { guild: string; data: ControlData }) {
  const t = words(data.strings);
  const { save } = useCommandControl(guild);
  const discord = useDiscord(guild);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<ControlCommand | null>(null);
  const [alias, setAlias] = useState("");
  const [target, setTarget] = useState("");
  const commands = filteredCommands(data.commands, search);
  return (
    <div className="settings-page command-control-page">
      <PageHead title={t("ui.title")} lead={t("ui.lead")} />
      <Field label={t("ui.search")} htmlFor="command-search">
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
          {t("saved")}
        </p>
      )}
      <div className="command-control-list" role="region" aria-label={t("ui.title")} tabIndex={0}>
        {commands.length === 0 && <p className="empty-row muted">{t("ui.empty")}</p>}
        {commands.map((command) => (
          <div key={command.path} className="command-row">
            <button
              type="button"
              className="command-open"
              onClick={() => setSelected(command)}
              aria-label={`${t("ui.edit")}: /${command.path}`}
            >
              <strong>/{command.path}</strong>
              <span className="muted">{command.description}</span>
            </button>
            {command.protected ? (
              <span className="command-protected">{t("ui.protected")}</span>
            ) : (
              <label className="command-switch">
                <input
                  type="checkbox"
                  checked={data.config.rules[command.path]?.enabled ?? true}
                  aria-label={`${t("ui.enabled")}: /${command.path}`}
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
                <span>{t("ui.enabled")}</span>
              </label>
            )}
          </div>
        ))}
      </div>
      <Panel title={t("ui.aliases")}>
        <p className="muted">{t("ui.alias_hint")}</p>
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
          <Field label={t("ui.alias_name")} htmlFor="alias">
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
          <Field label={t("ui.target")} htmlFor="alias-command">
            <select
              id="alias-command"
              className="pixel-input"
              required
              value={target}
              onChange={(e) => setTarget(e.target.value)}
            >
              <option value="">{t("ui.target")}</option>
              {data.commands.map((c) => (
                <option key={c.path}>{c.path}</option>
              ))}
            </select>
          </Field>
          <PixelButton
            type="submit"
            disabled={save.isPending || Object.keys(data.config.aliases).length >= 50}
          >
            {t("ui.add")}
          </PixelButton>
        </form>
        <p className="muted">{Object.keys(data.config.aliases).length}/50</p>
        {Object.keys(data.config.aliases).length === 0 && <p className="muted">{t("ui.no_aliases")}</p>}
        {Object.entries(data.config.aliases).map(([name, path]) => (
          <div className="command-alias-row" key={name}>
            <code>
              {name} → {path}
            </code>
            <PixelButton
              variant="ghost"
              disabled={save.isPending}
              aria-label={`${t("ui.remove")}: ${name}`}
              onClick={() => {
                const aliases = { ...data.config.aliases };
                delete aliases[name];
                save.mutate({ ...data.config, aliases });
              }}
            >
              {t("ui.remove")}
            </PixelButton>
          </div>
        ))}
      </Panel>
      <Panel title={t("ui.permissions")}>
        <p className="muted">{t("ui.permissions_hint")}</p>
        {data.permissions ? (
          <div className="command-permissions">
            {data.permissions.length === 0 && <p className="muted">{t("ui.no_permissions")}</p>}
            {data.permissions.map((entry) => (
              <section key={entry.id}>
                <h3>{data.permission_names[entry.id] ?? t("ui.command_id", { id: entry.id })}</h3>
                {entry.permissions.map((permission) => {
                  const kind = permission.type === 1 ? "role" : permission.type === 3 ? "channel" : "member";
                  const name =
                    kind === "role"
                      ? discord.data?.roles.find((role) => role.id === permission.id)?.name
                      : kind === "channel"
                        ? discord.data?.channels.find((channel) => channel.id === permission.id)?.name
                        : undefined;
                  return (
                    <div className="command-alias-row" key={`${kind}:${permission.id}`}>
                      <span>
                        {t(`ui.${kind}`)}: {name ?? permission.id}
                      </span>
                      <strong>{t(permission.permission ? "ui.allow" : "ui.deny")}</strong>
                    </div>
                  );
                })}
              </section>
            ))}
          </div>
        ) : (
          <p className="muted">{t("ui.unavailable")}</p>
        )}
      </Panel>
      {selected && (
        <CommandDrawer
          key={selected.path}
          command={selected}
          initial={data.config.rules[selected.path] ?? defaultRule()}
          info={discord.data}
          t={t}
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
    </div>
  );
}
function CommandDrawer({
  command,
  initial,
  info,
  t,
  pending,
  error,
  onClose,
  onSave,
}: {
  command: ControlCommand;
  initial: CommandRule;
  info: GuildInfo | undefined;
  t: Words;
  pending: boolean;
  error: string | undefined;
  onClose: () => void;
  onSave: (rule: CommandRule) => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [rule, setRule] = useState(initial);
  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);
  return (
    <dialog
      className="command-drawer"
      ref={dialog}
      onCancel={(e) => {
        if (pending) e.preventDefault();
        else onClose();
      }}
      aria-labelledby="command-drawer-title"
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          onSave(rule);
        }}
      >
        <header>
          <h2 id="command-drawer-title">/{command.path}</h2>
          <PixelButton variant="ghost" onClick={onClose} disabled={pending}>
            {t("ui.close")}
          </PixelButton>
        </header>
        <p className="muted">{command.description}</p>
        <p className="notice">{t("ui.inherited")}</p>
        {command.permissions !== "0" && <p className="muted">{t("ui.staff_checks")}</p>}
        <fieldset disabled={command.protected || pending} className="command-fields">
          <label className="command-switch">
            <input
              type="checkbox"
              checked={rule.enabled}
              onChange={(e) => setRule({ ...rule, enabled: e.target.checked })}
            />
            {t("ui.enabled")}
          </label>
          {(["allowed_roles", "denied_roles", "allowed_channels", "denied_channels"] as const).map(
            (field) => (
              <Field key={field} label={t(`ui.${field}`)} htmlFor={`command-${field}`}>
                <select
                  id={`command-${field}`}
                  className="pixel-input command-multi"
                  multiple
                  value={rule[field]}
                  disabled={!info}
                  onChange={(e) =>
                    setRule({ ...rule, [field]: Array.from(e.target.selectedOptions, (o) => o.value) })
                  }
                >
                  {(field.includes("roles") ? info?.roles : info?.channels)?.map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.name}
                    </option>
                  ))}
                </select>
              </Field>
            ),
          )}
          <p className="muted">{t("ui.zero_hint")}</p>
          {(
            [
              ["cooldown_secs", "cooldown"],
              ["delete_reply_secs", "delete_reply"],
              ["delete_invocation_secs", "delete_invocation"],
            ] as const
          ).map(([field, key]) => (
            <Field key={field} label={t(`ui.${key}`)} htmlFor={`command-${field}`}>
              <input
                className="pixel-input"
                id={`command-${field}`}
                type="number"
                min={0}
                max={field === "delete_reply_secs" ? 840 : 86400}
                required
                value={rule[field]}
                onChange={(e) => setRule({ ...rule, [field]: Number(e.target.value) })}
              />
            </Field>
          ))}
          <label className="command-switch">
            <input
              type="checkbox"
              checked={rule.ephemeral}
              disabled={!command.ephemeral_supported}
              onChange={(e) => setRule({ ...rule, ephemeral: e.target.checked })}
            />
            {t("ui.ephemeral")}
          </label>
          {!command.ephemeral_supported && <p className="muted">{t("no_ephemeral")}</p>}
        </fieldset>
        {error && (
          <p className="field-error" role="alert">
            {error}
          </p>
        )}
        {command.protected ? (
          <p className="notice">{t("protected")}</p>
        ) : (
          <footer>
            <PixelButton type="submit" disabled={pending}>
              {t(pending ? "ui.loading" : "ui.save")}
            </PixelButton>
          </footer>
        )}
      </form>
    </dialog>
  );
}
