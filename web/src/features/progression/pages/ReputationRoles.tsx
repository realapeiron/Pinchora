import { translate as spaTranslate } from "../../../i18n";
import { useState } from "react";
import { Field } from "../../../components/Field";
import { RolePicker, RoleTag } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { RichText } from "../../../components/RichText";
import { SettingsRow } from "../../../components/SettingsRow";
import { t } from "../../../i18n/dashboard";
import { levelingSummary } from "./levelingSummary";
import type { GuildInfo } from "../../../lib/api";
import { LIMITS, checkReputation, problem } from "../../../lib/rules";
import { useRepRoles } from "../../../lib/queries";

/** The message of a failed list write, for the add form. */
function writeError(error: unknown): string | null {
  return error instanceof Error ? error.message : null;
}

export function RepRoles({ guild, info }: { guild: string; info: GuildInfo | undefined }) {
  const { query, write } = useRepRoles(guild);
  const [reputation, setReputation] = useState("10");
  const [role, setRole] = useState<string | null>(null);
  const picked = info?.roles.find((r) => r.id === role);
  const local =
    reputation === "" ? spaTranslate("Write a count.") : problem(checkReputation(Number(reputation)));
  const items = query.data?.items ?? [];
  return (
    <SettingsRow
      rowId="reputation-roles-reputation-roles"
      title={t("row.Reputation roles", {}, "REPUTATION ROLES")}
      description={t("Give roles for reputation milestones.")}
      summary={levelingSummary({}, { repRoles: items.length }).repRoles}
    >
      <div className="settings-detail-fields">
        <span className="pixel-label muted">
          {t("{count} of {max} reputation roles used", {
            count: items.length,
            max: query.data?.max ?? LIMITS.max_rep_roles,
          })}
        </span>
        <p className="muted panel-lead">
          {" "}
          {spaTranslate(
            "Roles members get the moment their reputation reaches a count. They keep every one.",
          )}{" "}
        </p>
        {items.length === 0 ? (
          <p className="empty-row muted">{spaTranslate("No reputation roles yet.")}</p>
        ) : (
          <ul className="list" aria-label={spaTranslate("Reputation roles")}>
            {items.map((row) => (
              <li key={row.role_id} className="list-row">
                <span className="list-level pixel-label">
                  {row.reputation} {spaTranslate("REP")}
                </span>
                <span className="list-main">
                  <RoleTag info={info} id={row.role_id} />
                </span>
                <PixelButton
                  variant="ghost"
                  className="button-small"
                  aria-label={spaTranslate("Remove reputation role")}
                  disabled={write.isPending}
                  onClick={() => write.mutate({ method: "DELETE", sub: `/${row.role_id}` })}
                >
                  {spaTranslate("Remove")}
                </PixelButton>
              </li>
            ))}
          </ul>
        )}
        <div className="add-row add-row-role">
          <Field label={spaTranslate("Reputation")} htmlFor="rep-role-count" error={local}>
            <input
              id="rep-role-count"
              className="pixel-input input-short"
              type="number"
              min={LIMITS.reputation[0]}
              max={LIMITS.reputation[1]}
              value={reputation}
              onChange={(event) => setReputation(event.target.value)}
            />
          </Field>
          <Field
            label={spaTranslate("Role")}
            htmlFor="rep-role-role"
            error={picked && !picked.assignable ? picked.why_not : null}
          >
            <RolePicker id="rep-role-role" info={info} value={role} onChange={setRole} assignable />
          </Field>
          <div className="add-actions">
            <PixelButton
              disabled={!role || !!local || (picked && !picked.assignable) || write.isPending}
              onClick={() =>
                role &&
                write.mutate(
                  { method: "PUT", sub: `/${role}`, body: { reputation: Number(reputation) } },
                  { onSuccess: () => setRole(null) },
                )
              }
            >
              {" "}
              {spaTranslate("Add")}{" "}
            </PixelButton>
          </div>
        </div>
        {write.isError && (
          <p className="field-error" role="alert">
            <RichText text={writeError(write.error) ?? ""} />
          </p>
        )}
      </div>
    </SettingsRow>
  );
}
