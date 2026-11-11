import { useEffect, useRef, useState } from "react";
import type { GuildInfo } from "../../../lib/api";
import { Field } from "../../../components/Field";
import { PixelButton } from "../../../components/PixelButton";
import type { Words } from "../../moderation";
import { defaultRule, type ControlCommand, type CommandRule } from "../api/commandControl";

export function CommandDrawer({
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
        {Object.keys(rule.constraints ?? {}).length > 0 && (
          <div className="notice">
            <p>{t("ui.migrated_constraints")}</p>
            {Object.entries(rule.constraints ?? {}).map(([path, inherited]) => (
              <p key={path}>
                <code>/{path}</code>: {t(inherited.enabled ? "ui.enabled" : "disabled")}
                {(["allowed_roles", "denied_roles", "allowed_channels", "denied_channels"] as const).map(
                  (field) =>
                    inherited[field].length > 0
                      ? ` · ${t(`ui.${field}`)}: ${inherited[field].join(", ")}`
                      : "",
                )}
              </p>
            ))}
            <PixelButton
              type="button"
              variant="ghost"
              disabled={pending}
              onClick={() => setRule(defaultRule())}
            >
              {t("ui.reset")}
            </PixelButton>
          </div>
        )}
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
