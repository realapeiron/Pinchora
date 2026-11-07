import { translate as spaTranslate } from "../i18n";
import { useState } from "react";

import { Choice } from "../components/Choice";
import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { RolePicker, RoleTag } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { RichText } from "../components/RichText";
import { ApiError, type GuildInfo } from "../lib/api";
import { COLOR_MODES, type ColorMode, type RolesView, isHex, useRolesWrite } from "../lib/roles";

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
    <Panel
      title="COLOUR ROLES"
      aside={<span className="pixel-label muted">{`${colors.roles.length}/${colors.max} roles made`}</span>}
    >
      <p className="muted panel-lead">
        Members colour their name with <code>/color pick</code>. Each colour is a role the bot makes the first
        time somebody picks it, and a member holds one at a time.
      </p>
      <SwitchRow
        title="Let members pick a colour"
        control={
          <PixelToggle
            label={spaTranslate("Colour roles")}
            checked={colors.enabled}
            disabled={write.isPending}
            onChange={(on) => save({ enabled: on })}
          />
        }
      >
        Off hides <code>/color</code> from members. Colours already given stay.
      </SwitchRow>

      <Field label="Which colours">
        <Choice
          name="Colour mode"
          value={colors.mode}
          options={COLOR_MODES}
          onChange={(mode) => save({ mode })}
          disabled={write.isPending}
        />
      </Field>

      <h3 className="setting-name" style={{ marginTop: 24 }}>
        The palette
      </h3>
      <p className="muted fine">
        The card colours. A green tick means the role exists; the others are made the first time somebody
        picks them. Select a colour to try it on a name.
      </p>
      <ul className="colour-grid" aria-label="Palette">
        {colors.palette.map((c) => (
          <li key={c.hex}>
            <button
              type="button"
              className={`colour-tile ${sample === c.hex ? "is-picked" : ""}`}
              aria-pressed={sample === c.hex}
              aria-label={`${c.name}, ${made.has(c.hex) ? "role made" : "made when picked"}`}
              onClick={() => setSample(c.hex)}
            >
              <span className="tone-swatch" style={{ background: c.hex }} aria-hidden="true" />
              <span>{c.name}</span>
              <span className={`colour-state ${made.has(c.hex) ? "is-made" : ""}`} aria-hidden="true">
                {made.has(c.hex) ? "✓ ROLE MADE" : "\u00a0"}
              </span>
            </button>
          </li>
        ))}
      </ul>
      <div
        className="colour-sample"
        role="img"
        aria-label={`A name in ${sample}`}
        aria-live="polite"
        style={{ marginTop: 12 }}
      >
        <span className="discord-avatar" aria-hidden="true" />
        <strong style={{ color: isHex(sample) ? sample : undefined }}>Captain Crab</strong>
        <span className="muted">· how the name looks on Discord&apos;s dark chat</span>
      </div>
      <div className="panel-foot">
        {missing > 0 ? (
          <span className="muted fine">{missing} palette colours have no role yet.</span>
        ) : (
          <span className="muted fine">Every palette colour has its role.</span>
        )}
        <PixelButton
          variant="secondary"
          disabled={write.isPending || missing === 0 || colors.roles.length >= colors.max}
          onClick={() => write.mutate({ method: "POST", sub: "/colors/create-missing" })}
        >
          {write.isPending && write.variables?.sub === "/colors/create-missing"
            ? "Making roles…"
            : "Create the missing roles"}
        </PixelButton>
      </div>
      {report && (
        <p className={`notice ${report.stopped ? "notice-heart" : ""}`} role="status">
          {report.created.length > 0
            ? `Made ${report.created.length} role${report.created.length === 1 ? "" : "s"}: ${report.created.join(", ")}. `
            : "Made no roles. "}
          {report.stopped && <RichText text={report.stopped} />}
        </p>
      )}

      <div className="form-grid form-grid-2">
        <Field
          label="Who may pick"
          htmlFor="colour-allow"
          error={error?.field === "allowed_roles" ? error.message : null}
          hint={
            colors.allowed_roles.length === 0
              ? "Everyone may. Add a role to limit it, for boosters, say."
              : `Only members with one of these roles. ${colors.allowed_roles.length}/${colors.max_allowed}`
          }
        >
          <div className="chip-row">
            <RolePicker
              id="colour-allow"
              info={info}
              value={allow}
              onChange={setAllow}
              none="Pick a role to add"
            />
            <PixelButton
              variant="secondary"
              disabled={!allow || colors.allowed_roles.includes(allow) || write.isPending}
              onClick={() => allow && save({ allowed_roles: [...colors.allowed_roles, allow] })}
            >
              Add
            </PixelButton>
          </div>
          {colors.allowed_roles.length > 0 && (
            <ul className="channel-chips" aria-label="Roles that may pick">
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
          label="Make new colour roles under"
          htmlFor="colour-anchor"
          error={
            (picked && !picked.assignable ? picked.why_not : null) ??
            (error?.field === "anchor_role_id" ? error.message : null)
          }
          hint="A colour only shows when no role above it has one. Pick a role below the bot, and put it above the roles that colour names."
        >
          <RolePicker
            id="colour-anchor"
            info={info}
            value={colors.anchor_role_id}
            onChange={(role) => save({ anchor_role_id: role })}
            none="None: new roles land at the bottom"
            assignable
            disabled={write.isPending}
          />
        </Field>
      </div>

      {colors.roles.length > 0 && (
        <>
          <h3 className="setting-name" style={{ marginTop: 24 }}>
            Colour roles made
          </h3>
          <ul className="list" aria-label={spaTranslate("Colour roles")}>
            {colors.roles.map((r) => (
              <li key={r.role_id} className="list-row">
                <span className="list-main colour-role-name">
                  <span className="role-dot" style={{ background: r.hex }} aria-hidden="true" />
                  <RoleTag info={info} id={r.role_id} />
                  <span className="muted list-note">{r.hex}</span>
                  {own.some((o) => o.role_id === r.role_id) && <span className="badge badge-dim">YOURS</span>}
                </span>
                <PixelButton
                  variant="ghost"
                  className="button-small"
                  aria-label={`Remove ${r.name} from the colours`}
                  disabled={write.isPending}
                  onClick={() =>
                    write.mutate({
                      method: "DELETE",
                      sub: `/colors/${r.role_id}${deleteOnDiscord ? "?delete_role=true" : ""}`,
                    })
                  }
                >
                  {spaTranslate("Remove")}
                </PixelButton>
              </li>
            ))}
          </ul>
          <label className="fine muted" style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={deleteOnDiscord}
              onChange={(event) => setDeleteOnDiscord(event.target.checked)}
            />
            Remove also deletes the role on Discord
          </label>
        </>
      )}
      {error && !["allowed_roles", "anchor_role_id"].includes(error.field ?? "") && (
        <p className="field-error" role="alert">
          <RichText text={error.message} />
        </p>
      )}
    </Panel>
  );
}
