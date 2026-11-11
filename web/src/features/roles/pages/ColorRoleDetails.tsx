import { plural as spaPlural, translate as spaTranslate } from "../../../i18n";
import { RoleTag } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { RichText } from "../../../components/RichText";
import type { GuildInfo } from "../../../lib/api";
import { type RolesAnswer, type RolesView, isHex, useRolesWrite } from "../api/roles";

type Colors = RolesView["colors"];
type Writer = ReturnType<typeof useRolesWrite>;

export function ColorPalette({
  colors,
  made,
  sample,
  setSample,
  missing,
  write,
  report,
}: {
  colors: Colors;
  made: Set<string>;
  sample: string;
  setSample: (value: string) => void;
  missing: number;
  write: Writer;
  report: RolesAnswer["report"];
}) {
  return (
    <>
      <div className="settings-detail-fields">
        <p className="muted fine">
          {" "}
          {spaTranslate(
            "The card colours. A green tick means the role exists; the others are made the first time somebody picks them. Select a colour to try it on a name.",
          )}{" "}
        </p>
        <ul className="colour-grid" aria-label={spaTranslate("Palette")}>
          {colors.palette.map((c) => (
            <li key={c.hex}>
              <button
                type="button"
                className={`colour-tile ${sample === c.hex ? "is-picked" : ""}`}
                aria-pressed={sample === c.hex}
                aria-label={`${c.name}, ${made.has(c.hex) ? spaTranslate("role made") : spaTranslate("made when picked")}`}
                onClick={() => setSample(c.hex)}
              >
                <span className="tone-swatch" style={{ background: c.hex }} aria-hidden="true" />
                <span>{c.name}</span>
                <span className={`colour-state ${made.has(c.hex) ? "is-made" : ""}`} aria-hidden="true">
                  {made.has(c.hex) ? spaTranslate("✓ ROLE MADE") : "\u00a0"}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </div>
      <div className="settings-detail-preview">
        <div
          className="colour-sample"
          role="img"
          aria-label={spaTranslate("A name in {arg1}", { arg1: sample })}
          aria-live="polite"
          style={{ marginTop: 12 }}
        >
          <span className="discord-avatar" aria-hidden="true" />
          <strong style={{ color: isHex(sample) ? sample : undefined }}>Captain Crab</strong>
          <span className="muted">{spaTranslate("· how the name looks on Discord's dark chat")}</span>
        </div>
      </div>
      <div className="settings-detail-fields">
        <div className="panel-foot">
          {missing > 0 ? (
            <span className="muted fine">
              {missing} {spaTranslate("palette colours have no role yet.")}
            </span>
          ) : (
            <span className="muted fine">{spaTranslate("Every palette colour has its role.")}</span>
          )}
          <PixelButton
            variant="secondary"
            disabled={write.isPending || missing === 0 || colors.roles.length >= colors.max}
            onClick={() => write.mutate({ method: "POST", sub: "/colors/create-missing" })}
          >
            {write.isPending && write.variables?.sub === "/colors/create-missing"
              ? spaTranslate("Making roles…")
              : spaTranslate("Create the missing roles")}
          </PixelButton>
        </div>
        {report && (
          <p className={`notice ${report.stopped ? "notice-heart" : ""}`} role="status">
            {report.created.length > 0
              ? spaPlural(
                  report.created.length,
                  { one: "Made {count} role: {names}. ", other: "Made {count} roles: {names}. " },
                  { names: report.created.join(", ") },
                )
              : spaTranslate("Made no roles. ")}
            {report.stopped && <RichText text={report.stopped} />}
          </p>
        )}
      </div>
    </>
  );
}

export function ColorMadeRoles({
  colors,
  info,
  own,
  write,
  deleteOnDiscord,
  setDeleteOnDiscord,
}: {
  colors: Colors;
  info: GuildInfo | undefined;
  own: Colors["roles"];
  write: Writer;
  deleteOnDiscord: boolean;
  setDeleteOnDiscord: (next: boolean) => void;
}) {
  return (
    <div className="settings-detail-fields">
      {colors.roles.length > 0 && (
        <>
          <ul className="list" aria-label={spaTranslate("Colour roles")}>
            {colors.roles.map((r) => (
              <li key={r.role_id} className="list-row">
                <span className="list-main colour-role-name">
                  <span className="role-dot" style={{ background: r.hex }} aria-hidden="true" />
                  <RoleTag info={info} id={r.role_id} />
                  <span className="muted list-note">{r.hex}</span>
                  {own.some((o) => o.role_id === r.role_id) && (
                    <span className="badge badge-dim">{spaTranslate("YOURS")}</span>
                  )}
                </span>
                <PixelButton
                  variant="ghost"
                  className="button-small"
                  aria-label={spaTranslate("Remove {arg1} from the colours", { arg1: r.name })}
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
            />{" "}
            {spaTranslate("Remove also deletes the role on Discord")}{" "}
          </label>
        </>
      )}
    </div>
  );
}
