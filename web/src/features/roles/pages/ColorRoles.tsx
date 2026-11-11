import { translate as spaTranslate } from "../../../i18n";
import { useState } from "react";

import { Choice } from "../../../components/Choice";
import { Field } from "../../../components/Field";
import { SettingsRow } from "../../../components/SettingsRow";
import { ColorPalette, ColorMadeRoles } from "./ColorRoleDetails";
import { colorRolesSummary } from "./roleSummary";
import { t } from "../../../i18n/dashboard";
import { RolePicker, RoleTag } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { RichText } from "../../../components/RichText";
import { ApiError, type GuildInfo } from "../../../lib/api";
import { COLOR_MODES, type ColorMode, type RolesView, useRolesWrite } from "../api/roles";

/** Colour roles: whether they are on, who may pick, the palette, and the roles the bot has made. */
export function ColorRoles({
  guild,
  info,
  view,
}: {
  guild: string;
  info: GuildInfo | undefined;
  view: RolesView;
}) {
  const write = useRolesWrite(guild);
  const colors = view.colors;
  const [sample, setSample] = useState<string>(colors.palette[0]?.hex ?? "#ff6b4a");
  const [allow, setAllow] = useState<string | null>(null);
  const [deleteOnDiscord, setDeleteOnDiscord] = useState(false);
  const made = new Set(colors.roles.map((r) => r.hex));
  const own = colors.roles.filter((r) => !colors.palette.some((p) => p.hex === r.hex));
  const error = write.error instanceof ApiError ? write.error : null;
  const picked = info?.roles.find((r) => r.id === colors.anchor_role_id);
  const report = write.data?.report;

  /** Saves the settings as they are now, with `patch` applied. */
  const save = (
    patch: Partial<{
      enabled: boolean;
      mode: ColorMode;
      allowed_roles: string[];
      anchor_role_id: string | null;
    }>,
  ) =>
    write.mutate({
      method: "PUT",
      sub: "/colors",
      body: {
        enabled: colors.enabled,
        mode: colors.mode,
        allowed_roles: colors.allowed_roles,
        anchor_role_id: colors.anchor_role_id,
        ...patch,
      },
    });

  const missing = colors.palette.filter((p) => !made.has(p.hex)).length;
  const name = (id: string) => info?.roles.find((r) => r.id === id)?.name ?? "a deleted role";

  return (
    <div className="settings-detail-list color-role-settings">
      <SettingsRow
        title={spaTranslate("COLOUR ROLES")}
        description={spaTranslate("Let members pick a colour")}
        summary={colorRolesSummary(colors)}
        enabled={colors.enabled}
        onToggle={(on) => save({ enabled: on })}
        toggleLabel={spaTranslate("Colour roles")}
        toggleDisabled={write.isPending}
      >
        <div className="settings-detail-fields">
          <span className="pixel-label muted">{`${colors.roles.length}/${colors.max} roles made`}</span>
          <p className="muted panel-lead">
            {" "}
            {spaTranslate("Members colour their name with")} <code>/color pick</code>
            {spaTranslate(
              ". Each colour is a role the bot makes the first time somebody picks it, and a member holds one at a time.",
            )}{" "}
          </p>
          <p className="muted fine">
            {spaTranslate("Off hides")} <code>/color</code>{" "}
            {spaTranslate("from members. Colours already given stay.")}
          </p>
          <Field label={spaTranslate("Which colours")}>
            <Choice
              name="Colour mode"
              value={colors.mode}
              options={COLOR_MODES}
              onChange={(mode) => save({ mode })}
              disabled={write.isPending}
            />
          </Field>

          <div className="form-grid form-grid-2">
            <Field
              label={spaTranslate("Who may pick")}
              htmlFor="colour-allow"
              error={error?.field === "allowed_roles" ? error.message : null}
              hint={
                colors.allowed_roles.length === 0
                  ? spaTranslate("Everyone may. Add a role to limit it, for boosters, say.")
                  : spaTranslate("Only members with one of these roles. {arg1}/{arg2}", {
                      arg1: colors.allowed_roles.length,
                      arg2: colors.max_allowed,
                    })
              }
            >
              <div className="chip-row">
                <RolePicker
                  id="colour-allow"
                  info={info}
                  value={allow}
                  onChange={setAllow}
                  none={spaTranslate("Pick a role to add")}
                />
                <PixelButton
                  variant="secondary"
                  disabled={!allow || colors.allowed_roles.includes(allow) || write.isPending}
                  onClick={() => allow && save({ allowed_roles: [...colors.allowed_roles, allow] })}
                >
                  {" "}
                  {spaTranslate("Add")}{" "}
                </PixelButton>
              </div>
              {colors.allowed_roles.length > 0 && (
                <ul className="channel-chips" aria-label={spaTranslate("Roles that may pick")}>
                  {colors.allowed_roles.map((id) => (
                    <li key={id} className="channel-chip">
                      <RoleTag info={info} id={id} />
                      <button
                        type="button"
                        className="chip-remove"
                        aria-label={`Stop letting ${name(id)} pick`}
                        onClick={() => save({ allowed_roles: colors.allowed_roles.filter((r) => r !== id) })}
                      >
                        ✕
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </Field>
            <Field
              label={spaTranslate("Make new colour roles under")}
              htmlFor="colour-anchor"
              error={
                (picked && !picked.assignable ? picked.why_not : null) ??
                (error?.field === "anchor_role_id" ? error.message : null)
              }
              hint={spaTranslate(
                "A colour only shows when no role above it has one. Pick a role below the bot, and put it above the roles that colour names.",
              )}
            >
              <RolePicker
                id="colour-anchor"
                info={info}
                value={colors.anchor_role_id}
                onChange={(role) => save({ anchor_role_id: role })}
                none={spaTranslate("None: new roles land at the bottom")}
                assignable
                disabled={write.isPending}
              />
            </Field>
          </div>
        </div>
      </SettingsRow>
      <SettingsRow
        title={spaTranslate("The palette")}
        description={t("Preview colours and create the missing roles.")}
        summary={t("{count} colours · {missing} not made", { count: colors.palette.length, missing })}
      >
        <ColorPalette
          colors={colors}
          made={made}
          sample={sample}
          setSample={setSample}
          missing={missing}
          write={write}
          report={report}
        />
      </SettingsRow>

      {colors.roles.length > 0 && (
        <SettingsRow
          title={spaTranslate("Colour roles made")}
          description={t("Review or remove created colour roles.")}
          summary={t("{count} roles made", { count: colors.roles.length })}
        >
          <ColorMadeRoles
            colors={colors}
            info={info}
            own={own}
            write={write}
            deleteOnDiscord={deleteOnDiscord}
            setDeleteOnDiscord={setDeleteOnDiscord}
          />
        </SettingsRow>
      )}
      {error && !["allowed_roles", "anchor_role_id"].includes(error.field ?? "") && (
        <p className="field-error" role="alert">
          <RichText text={error.message} />
        </p>
      )}
    </div>
  );
}
