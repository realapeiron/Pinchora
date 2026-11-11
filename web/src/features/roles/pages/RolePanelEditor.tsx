import { translate as spaTranslate } from "../../../i18n";
import { Choice } from "../../../components/Choice";
import { Field, SwitchRow } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { RolePicker } from "../../../components/Pickers";
import { PixelToggle } from "../../../components/PixelToggle";
import { ApiError, type GuildInfo } from "../../../lib/api";
import {
  LIMITS,
  STYLES,
  type Panel as PanelData,
  type PanelDraft,
  problemWithRules,
  reactionProblems,
  useRolesWrite,
} from "../api/roles";
import { RolePanelEntries } from "./RolePanelEntries";
import { PostRow } from "./RolePanelPost";
import { PanelPreview } from "./RolePanelPreview";

/** What a failed write said about `field`, if it named it. */
function said(error: ApiError | null, field: string): string | null {
  return error?.errors.find((e) => e.field === field)?.message ?? null;
}

export function Editor({
  info,
  draft,
  setDraft,
  saved,
  current,
  unsaved,
  problem,
  server,
  posting,
}: {
  info: GuildInfo | undefined;
  draft: PanelDraft;
  setDraft: (next: PanelDraft) => void;
  saved: PanelDraft;
  current: PanelData | null;
  unsaved: boolean;
  problem: { field: string; message: string } | null;
  server: ApiError | null;
  posting: ReturnType<typeof useRolesWrite>;
}) {
  const set = <K extends keyof PanelDraft>(key: K, value: PanelDraft[K]) =>
    setDraft({ ...draft, [key]: value });
  const setRow = (key: string, patch: Partial<PanelDraft["roles"][number]>) =>
    set(
      "roles",
      draft.roles.map((r) => (r.key === key ? { ...r, ...patch } : r)),
    );
  const move = (index: number, by: -1 | 1) => {
    const to = index + by;
    if (to < 0 || to >= draft.roles.length) return;
    const next = [...draft.roles];
    const [row] = next.splice(index, 1);
    if (row) next.splice(to, 0, row);
    set("roles", next);
  };
  const rules = problemWithRules(draft.exclusive, draft.min_picks, draft.max_picks);
  const number = (raw: string) => (raw === "" ? 0 : Math.max(0, Math.round(Number(raw)) || 0));
  const why = (field: string) => (problem?.field === field ? problem.message : null) ?? said(server, field);
  const warnings = reactionProblems(draft);

  return (
    <Panel
      title={current ? spaTranslate("EDIT PANEL") : spaTranslate("NEW PANEL")}
      aside={<span className="pixel-label muted">{`${draft.roles.length}/${LIMITS.entries} roles`}</span>}
    >
      <div className="panel-designer">
        <div className="designer-preview settings-detail-preview">
          <PanelPreview draft={draft} info={info} />
        </div>
        <div className="designer-controls">
          <div className="form-grid form-grid-2" style={{ marginTop: 0 }}>
            <Field
              label={spaTranslate("Title")}
              htmlFor="panel-title"
              error={why("title")}
              aside={
                <span className="counter">
                  {[...draft.title].length}/{LIMITS.title}
                </span>
              }
            >
              <input
                id="panel-title"
                className="pixel-input"
                aria-required="true"
                value={draft.title}
                maxLength={LIMITS.title + 20}
                placeholder={spaTranslate("Pick your colours")}
                onChange={(event) => set("title", event.target.value)}
                aria-invalid={why("title") ? true : undefined}
              />
            </Field>
            <Field
              label={spaTranslate("Text above the roles")}
              htmlFor="panel-description"
              error={why("description")}
              aside={
                <span
                  className={`counter ${[...draft.description].length > LIMITS.description ? "is-over" : ""}`}
                >
                  {[...draft.description].length}/{LIMITS.description}
                </span>
              }
            >
              <textarea
                id="panel-description"
                className="pixel-input pixel-textarea"
                rows={3}
                value={draft.description}
                placeholder={spaTranslate("What are these roles for?")}
                onChange={(event) => set("description", event.target.value)}
                aria-invalid={why("description") ? true : undefined}
              />
            </Field>
          </div>

          <Field label={spaTranslate("Style")}>
            <Choice
              name="Panel style"
              value={draft.style}
              options={STYLES}
              onChange={(style) => set("style", style)}
            />
          </Field>

          <div className="panel-rules">
            <Field
              label={spaTranslate("Required role")}
              htmlFor="panel-required"
              hint={spaTranslate("Only members with it can use the panel.")}
            >
              <RolePicker
                id="panel-required"
                info={info}
                value={draft.required_role}
                onChange={(role) => set("required_role", role)}
                none={spaTranslate("Anyone")}
              />
            </Field>
            <Field
              label={spaTranslate("Fewest roles")}
              htmlFor="panel-min"
              hint={spaTranslate("They keep at least this many.")}
            >
              <input
                id="panel-min"
                className="pixel-input input-short"
                type="number"
                min={0}
                max={LIMITS.entries}
                value={draft.min_picks}
                onChange={(event) => set("min_picks", number(event.target.value))}
                aria-invalid={rules || why("rules") ? true : undefined}
              />
            </Field>
            <Field
              label={spaTranslate("Most roles")}
              htmlFor="panel-max"
              hint={spaTranslate("0 has no limit.")}
            >
              <input
                id="panel-max"
                className="pixel-input input-short"
                type="number"
                min={0}
                max={LIMITS.entries}
                value={draft.max_picks}
                disabled={draft.exclusive}
                onChange={(event) => set("max_picks", number(event.target.value))}
                aria-invalid={rules || why("rules") ? true : undefined}
              />
            </Field>
          </div>
          {(rules ?? why("rules") ?? said(server, "min_picks") ?? said(server, "max_picks")) && (
            <p className="field-error" role="alert">
              {rules ?? why("rules") ?? said(server, "min_picks") ?? said(server, "max_picks")}
            </p>
          )}
          <SwitchRow
            title={spaTranslate("One at a time")}
            control={
              <PixelToggle
                label={spaTranslate("One at a time")}
                checked={draft.exclusive}
                onChange={(on) => setDraft({ ...draft, exclusive: on, max_picks: on ? 0 : draft.max_picks })}
              />
            }
          >
            {" "}
            {spaTranslate(
              "Picking a role takes the member's other role from this panel away, like a team or a colour.",
            )}{" "}
          </SwitchRow>

          <RolePanelEntries
            draft={draft}
            saved={saved}
            info={info}
            why={why}
            warnings={warnings}
            setRoles={(roles) => set("roles", roles)}
            setRow={setRow}
            move={move}
          />

          {current && (
            <PostRow
              info={info}
              panel={current}
              unsaved={unsaved}
              posting={posting}
              blocked={warnings.length > 0 || draft.roles.length === 0}
            />
          )}
          {!current && (
            <p className="muted fine">{spaTranslate("Save the panel first, then post it in a channel.")}</p>
          )}
        </div>
      </div>
    </Panel>
  );
}
