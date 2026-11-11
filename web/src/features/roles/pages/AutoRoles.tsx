import { translate as spaTranslate } from "../../../i18n";
import { useState } from "react";
import { Choice } from "../../../components/Choice";
import { DurationInput } from "../components/DurationInput";
import { Field } from "../../../components/Field";
import { RolePicker, RoleTag } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { PixelToggle } from "../../../components/PixelToggle";
import { RichText } from "../../../components/RichText";
import { Toast } from "../../../components/Toast";
import { SettingsRow } from "../../../components/SettingsRow";
import { ApiError, type GuildInfo } from "../../../lib/api";
import {
  type AutoKind,
  type AutoRole,
  type RolesView,
  problemWithDelay,
  problemWithExpiry,
  spanText,
  useRolesWrite,
} from "../api/roles";
import { autoRolesSummary } from "./roleSummary";
import { StickyRoleRow } from "./StickyRoleRow";
import { KINDS, kindName, Timeline } from "./AutoRoleTimeline";
import { t } from "../../../i18n/dashboard";

/** The sentence a failed write ends in, for the field it names. */
function problemOf(error: unknown, field: string): string | null {
  return error instanceof ApiError && error.errors.some((e) => e.field === field)
    ? (error.errors.find((e) => e.field === field)?.message ?? null)
    : null;
}

export function AutoRoles({
  guild,
  info,
  view,
}: {
  guild: string;
  info: GuildInfo | undefined;
  view: RolesView;
}) {
  const write = useRolesWrite(guild);
  const [role, setRole] = useState<string | null>(null);
  const [kind, setKind] = useState<AutoKind>("join");
  const [delay, setDelay] = useState(0);
  const [expiry, setExpiry] = useState(0);
  const [fresh, setFresh] = useState(0);
  const [toast, setToast] = useState<string | null>(null);
  const picked = info?.roles.find((r) => r.id === role);
  const { items, max, sticky } = view.auto;
  const delayProblem = problemWithDelay(delay);
  const expiryProblem = problemWithExpiry(expiry);
  const refused = picked && !picked.assignable ? picked.why_not : null;
  const roleName = (id: string) => info?.roles.find((r) => r.id === id)?.name ?? "a deleted role";

  const save = (
    row: Pick<AutoRole, "role_id" | "kind" | "delay_secs" | "remove_after_secs" | "enabled">,
    done?: () => void,
  ) =>
    write.mutate(
      {
        method: "PUT",
        sub: `/auto/${row.role_id}/${row.kind}`,
        body: { delay_secs: row.delay_secs, remove_after_secs: row.remove_after_secs, enabled: row.enabled },
      },
      { onSuccess: done },
    );

  return (
    <div className="settings-detail-list">
      <SettingsRow
        title={spaTranslate("AUTO ROLES")}
        description={t("Give roles to members and bots when they join.")}
        summary={autoRolesSummary(items)}
        initialOpen
      >
        <div className="settings-detail-fields">
          <span className="pixel-label muted">{`${items.length}/${max}`}</span>
          <p className="muted panel-lead">
            {" "}
            {spaTranslate(
              "Give a role to everyone who joins, with a delay if you like, and take it back after a while. Adding the same role and kind again changes it.",
            )}{" "}
          </p>
          {items.length === 0 ? (
            <p className="empty-row muted">{spaTranslate("No auto roles yet.")}</p>
          ) : (
            <ul className="list" aria-label={spaTranslate("Auto roles")}>
              {items.map((row) => (
                <li
                  key={`${row.role_id}:${row.kind}`}
                  className={`list-row auto-row ${row.enabled ? "" : "is-paused"}`}
                >
                  <span className="list-main">
                    <RoleTag info={info} id={row.role_id} />
                    <span className="auto-meta">
                      <span className="badge badge-ink">{kindName(row.kind).toUpperCase()}</span>
                      {row.delay_secs > 0 && (
                        <span className="muted">
                          {spaTranslate("after")} {spanText(row.delay_secs)}
                        </span>
                      )}
                      {row.remove_after_secs > 0 && (
                        <span className="muted">
                          {spaTranslate("taken back after")} {spanText(row.remove_after_secs)}
                        </span>
                      )}
                      {!row.enabled && <span className="badge badge-heart">{spaTranslate("PAUSED")}</span>}
                    </span>
                  </span>
                  <PixelToggle
                    label={spaTranslate("Give {arg1} to {arg2}", {
                      arg1: roleName(row.role_id),
                      arg2: kindName(row.kind).toLowerCase(),
                    })}
                    checked={row.enabled}
                    disabled={write.isPending}
                    onChange={(on) =>
                      save({
                        role_id: row.role_id,
                        kind: row.kind,
                        delay_secs: row.delay_secs,
                        remove_after_secs: row.remove_after_secs,
                        enabled: on,
                      })
                    }
                  />
                  <PixelButton
                    variant="ghost"
                    className="button-small"
                    aria-label={spaTranslate("Remove {arg1} from {arg2}", {
                      arg1: roleName(row.role_id),
                      arg2: kindName(row.kind).toLowerCase(),
                    })}
                    disabled={write.isPending}
                    onClick={() =>
                      write.mutate({ method: "DELETE", sub: `/auto/${row.role_id}/${row.kind}` })
                    }
                  >
                    {spaTranslate("Remove")}
                  </PixelButton>
                </li>
              ))}
            </ul>
          )}

          <div className="auto-form">
            <Field label={spaTranslate("Who gets it")}>
              <Choice
                name={spaTranslate("Who gets the role")}
                value={kind}
                options={KINDS.map((item) => ({
                  ...item,
                  title: spaTranslate(item.title),
                  detail: spaTranslate(item.detail),
                }))}
                onChange={setKind}
              />
            </Field>
            <div className="auto-fields">
              <Field
                label={spaTranslate("Role")}
                htmlFor="auto-role"
                error={refused ?? problemOf(write.error, "role_id")}
                hint={spaTranslate("The bot's role must sit above it.")}
              >
                <RolePicker id="auto-role" info={info} value={role} onChange={setRole} assignable />
              </Field>
              <Field
                label={spaTranslate("Wait before giving it")}
                htmlFor="auto-delay"
                error={delayProblem ?? problemOf(write.error, "delay_secs")}
                hint={spaTranslate("0 gives it at once.")}
              >
                <DurationInput
                  key={`d${fresh}`}
                  id="auto-delay"
                  label={spaTranslate("Wait before giving it")}
                  value={delay}
                  onChange={setDelay}
                  invalid={!!delayProblem}
                />
              </Field>
              <Field
                label={spaTranslate("Take it back after")}
                htmlFor="auto-expiry"
                error={expiryProblem ?? problemOf(write.error, "remove_after_secs")}
                hint={spaTranslate("0 keeps it for good.")}
              >
                <DurationInput
                  key={`e${fresh}`}
                  id="auto-expiry"
                  label={spaTranslate("Take it back after")}
                  value={expiry}
                  onChange={setExpiry}
                  invalid={!!expiryProblem}
                />
              </Field>
              <div className="add-actions">
                <PixelButton
                  disabled={!role || !!refused || !!delayProblem || !!expiryProblem || write.isPending}
                  onClick={() =>
                    role &&
                    save(
                      { role_id: role, kind, delay_secs: delay, remove_after_secs: expiry, enabled: true },
                      () => {
                        setRole(null);
                        setDelay(0);
                        setExpiry(0);
                        setFresh((n) => n + 1);
                        setToast(spaTranslate("Auto role saved."));
                      },
                    )
                  }
                >
                  {items.some((r) => r.role_id === role && r.kind === kind)
                    ? spaTranslate("Update")
                    : spaTranslate("Add")}
                </PixelButton>
              </div>
            </div>
          </div>
          {write.isError &&
            !problemOf(write.error, "role_id") &&
            !problemOf(write.error, "delay_secs") &&
            !problemOf(write.error, "remove_after_secs") && (
              <p className="field-error" role="alert">
                <RichText text={write.error instanceof Error ? write.error.message : ""} />
              </p>
            )}

          <h3 className="setting-name" style={{ marginTop: 24 }}>
            {" "}
            {spaTranslate("What happens when someone joins")}{" "}
          </h3>
          <Timeline items={items} info={info} />
        </div>
      </SettingsRow>
      <StickyRoleRow
        sticky={sticky}
        pending={write.isPending}
        onChange={(on) => write.mutate({ method: "PUT", sub: "/sticky", body: { enabled: on } })}
      />
      <Toast message={toast} onDone={() => setToast(null)} />
    </div>
  );
}
