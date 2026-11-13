import { t as deferredText } from "../../../i18n/dashboard";
import { useState } from "react";
import { useOutletContext } from "react-router";
import { Field } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dashboardT } from "../../../i18n/dashboard";
import { accessSummary } from "./accessSummary";
import { RolePicker, RoleTag } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { SaveBar } from "../../../components/SaveBar";
import { Toast } from "../../../components/Toast";
import { translate as t } from "../../../i18n";
import { useDiscord } from "../../../lib/queries";
import {
  areaLabel,
  canonicalGrants,
  useAccess,
  type AccessOverview,
  type AccessView,
  type ManagerGrant,
} from "../../moderation";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import "../../../styles/server-tools.css";

export function Access() {
  const data = useOutletContext<AccessOverview>();
  const access = useAccess(data.guild.id);
  if (access.query.isPending) return <Loading label={deferredText("Loading access…")} />;
  if (access.query.isError) return <Problem error={access.query.error} />;
  return (
    <AccessEditor key={data.guild.id} guild={data.guild.id} initial={access.query.data} access={access} />
  );
}
function AccessEditor({
  guild,
  initial,
  access,
}: {
  guild: string;
  initial: AccessView;
  access: ReturnType<typeof useAccess>;
}) {
  const discord = useDiscord(guild);
  const [saved, setSaved] = useState(initial.items);
  const [draft, setDraft] = useState<ManagerGrant[]>(initial.items);
  const [role, setRole] = useState<string | null>(null);
  const [preview, setPreview] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const dirty = JSON.stringify(canonicalGrants(saved)) !== JSON.stringify(canonicalGrants(draft));
  const invalid = draft.some((r) => r.areas.length === 0);
  const picked = discord.data?.roles.find((r) => r.id === role);
  const canAdd =
    picked &&
    !picked.everyone &&
    !picked.managed &&
    !draft.some((r) => r.role_id === role) &&
    draft.length < initial.max;
  return (
    <SettingsPage
      title={t("ACCESS")}
      lead={deferredText(
        "Give server roles access to chosen dashboard areas. Discord permissions and role safety still apply.",
      )}
      action={
        <PixelButton variant="secondary" onClick={() => setPreview(!preview)} aria-expanded={preview}>
          {deferredText("Preview access")}
        </PixelButton>
      }
    >
      <p className="notice">
        {deferredText(
          "The owner, Administrators and members with Manage Server always have full access. Only they can change access, backups and command controls.",
        )}
      </p>
      <div className="settings-detail-list">
        <SettingsRow
          rowId="access-manager-roles"
          title={dashboardT("Manager roles", {}, "MANAGER ROLES")}
          description={dashboardT("Manage roles that can open dashboard areas.")}
          summary={accessSummary(draft)}
          initialOpen
        >
          <div className="settings-detail-fields">
            <span className="pixel-label muted">
              {dashboardT("{count} of {max} manager roles used", { count: draft.length, max: initial.max })}
            </span>
            <p className="muted panel-lead">
              {deferredText(
                "Grants from all held roles combine. Removing a grant or a Discord role takes effect on the next request.",
              )}
            </p>
            {discord.isError && <Problem error={discord.error} />}
            {draft.length === 0 && (
              <p className="empty-row muted">
                {deferredText("No delegated roles. Only Discord server managers can use this dashboard.")}
              </p>
            )}
            <ul className="access-grants">
              {draft.map((grant) => (
                <li key={grant.role_id} className="access-grant">
                  <div className="access-role">
                    <RoleTag info={discord.data} id={grant.role_id} />
                    <PixelButton
                      variant="ghost"
                      disabled={access.save.isPending}
                      aria-label={deferredText("Remove access for {role}", {
                        role: discord.data?.roles.find((r) => r.id === grant.role_id)?.name ?? grant.role_id,
                      })}
                      onClick={() => setDraft(draft.filter((r) => r.role_id !== grant.role_id))}
                    >
                      {t("Remove")}
                    </PixelButton>
                  </div>
                  <fieldset className="access-areas" disabled={access.save.isPending}>
                    <legend>{deferredText("Allowed areas")}</legend>
                    {initial.areas.map((area) => (
                      <label key={area}>
                        <input
                          type="checkbox"
                          checked={grant.areas.includes(area)}
                          onChange={(e) =>
                            setDraft(
                              draft.map((r) =>
                                r.role_id === grant.role_id
                                  ? {
                                      ...r,
                                      areas: e.target.checked
                                        ? [...r.areas, area]
                                        : r.areas.filter((a) => a !== area),
                                    }
                                  : r,
                              ),
                            )
                          }
                        />
                        {areaLabel(area)}
                      </label>
                    ))}
                  </fieldset>
                  {grant.areas.length === 0 && (
                    <p className="field-error">
                      {deferredText("Choose at least one area or remove this role.")}
                    </p>
                  )}
                </li>
              ))}
            </ul>
            <p className="muted fine">
              {deferredText(
                "Configuration includes welcome, leveling, points, modules, branding, custom commands and responders. Audit access includes all server dashboard changes.",
              )}
            </p>
            <div className="add-row">
              <Field label={t("Role")} htmlFor="access-role">
                <RolePicker
                  id="access-role"
                  info={discord.data}
                  value={role}
                  onChange={setRole}
                  none={t("Pick a role")}
                  disabled={access.save.isPending}
                />
              </Field>
              <PixelButton
                disabled={!canAdd || access.save.isPending}
                onClick={() => {
                  if (role && canAdd) {
                    setDraft([...draft, { role_id: role, areas: [] }]);
                    setRole(null);
                  }
                }}
              >
                {deferredText("Add manager role")}
              </PixelButton>
            </div>
            {access.save.isError && <Problem error={access.save.error} />}
          </div>
          {preview && (
            <div className="settings-detail-preview">
              <Panel title={deferredText("ACCESS PREVIEW")}>
                <p className="muted">
                  {deferredText("Unsaved preview. Each role can open these areas; other pages are denied.")}
                </p>
                {draft.map((r) => (
                  <p className="access-preview" key={r.role_id}>
                    <RoleTag info={discord.data} id={r.role_id} />
                    <span>{r.areas.map(areaLabel).join(" · ") || deferredText("No areas selected")}</span>
                  </p>
                ))}
              </Panel>
            </div>
          )}
        </SettingsRow>
      </div>
      <SaveBar
        count={dirty ? 1 : 0}
        saving={access.save.isPending}
        blocked={invalid}
        onReset={() => {
          setDraft(saved);
          access.save.reset();
        }}
        onSave={() =>
          access.save.mutate(draft, {
            onSuccess: (data) => {
              setSaved(data.items);
              setDraft(data.items);
              setToast(deferredText("Access saved."));
            },
          })
        }
      />
      <Toast message={toast} onDone={() => setToast(null)} />
    </SettingsPage>
  );
}
