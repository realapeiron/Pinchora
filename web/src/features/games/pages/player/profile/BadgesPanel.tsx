import { Panel } from "../../../../../components/Panel";
import { t, number } from "../../../../../i18n";
import type { ProfileEditorModel } from "./useProfileEditor";
import { PixelButton } from "../../../../../components/PixelButton";

export function BadgesPanel({
  model,
}: {
  model: Pick<
    ProfileEditorModel,
    "data" | "draft" | "setDraft" | "dragged" | "setDragged" | "move" | "pick" | "buy"
  >;
}) {
  const { data, draft, setDraft, dragged, setDragged, move, pick, buy } = model;
  return (
    <Panel title={t("Your badge collection")} aside={<span>{draft.badges.length}/6</span>}>
      <label className="player-check">
        <input
          type="checkbox"
          checked={!draft.badges_set}
          onChange={(e) =>
            setDraft((p) => ({
              ...p,
              badges_set: !e.target.checked,
              badges: e.target.checked ? [] : p.badges,
            }))
          }
        />
        {t("Automatically show my best badges")}
      </label>
      <p className="muted">
        {t(
          "Choose up to six. Drag selected badges to reorder, or use the move buttons and Alt + arrow keys.",
        )}
      </p>
      <ol className="player-badge-order" aria-label={t("Badge order")}>
        {draft.badges.map((id, i) => {
          const badge = data.badges.find((b) => b.id === id);
          return (
            <li
              key={id}
              draggable
              onDragStart={() => setDragged(i)}
              onDragEnd={() => setDragged(null)}
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => {
                e.preventDefault();
                if (dragged !== null) move(dragged, i);
                setDragged(null);
              }}
            >
              <button
                type="button"
                className="player-badge-handle"
                aria-label={t("Reorder {name}", { name: badge?.name ?? id })}
                onKeyDown={(e) => {
                  if (!e.altKey) return;
                  if (e.key === "ArrowUp" && i > 0) {
                    e.preventDefault();
                    move(i, i - 1);
                  }
                  if (e.key === "ArrowDown" && i < draft.badges.length - 1) {
                    e.preventDefault();
                    move(i, i + 1);
                  }
                }}
              >
                {i + 1}. {badge?.name ?? id}
              </button>
              <button
                type="button"
                disabled={i === 0}
                onClick={() => move(i, i - 1)}
                aria-label={t("Move {name} earlier", { name: badge?.name ?? id })}
              >
                {t("Earlier")}
              </button>
              <button
                type="button"
                disabled={i === draft.badges.length - 1}
                onClick={() => move(i, i + 1)}
                aria-label={t("Move {name} later", { name: badge?.name ?? id })}
              >
                {t("Later")}
              </button>
              <button
                type="button"
                onClick={() => pick(id)}
                aria-label={t("Remove {name}", { name: badge?.name ?? id })}
              >
                {t("Remove")}
              </button>
            </li>
          );
        })}
      </ol>
      <div className="player-collection">
        {data.badges.map((badge) => (
          <article
            key={badge.id}
            className={`player-collectible ${draft.badges.includes(badge.id) && draft.badges_set ? "is-picked" : ""}`}
          >
            <img src={`/api/me/art/badge/${badge.id}`} alt="" className="player-badge-art" loading="lazy" />
            <h3>{badge.name}</h3>
            <p className="muted">{badge.description}</p>
            {badge.new && <span className="pixel-label">{t("New")}</span>}
            {badge.progress !== null && badge.target !== null && (
              <>
                <progress
                  aria-label={badge.name}
                  max={badge.target}
                  value={Math.min(badge.progress, badge.target)}
                />
                <small>
                  {number(badge.progress)} / {number(badge.target)} {badge.unit}
                </small>
              </>
            )}
            {badge.earned ? (
              <PixelButton
                variant="secondary"
                aria-pressed={draft.badges.includes(badge.id) && draft.badges_set}
                disabled={!draft.badges.includes(badge.id) && draft.badges.length >= 6}
                onClick={() => pick(badge.id)}
              >
                {draft.badges.includes(badge.id) ? t("Remove") : t("Select badge")}
              </PixelButton>
            ) : badge.price ? (
              <PixelButton
                variant="secondary"
                disabled={buy.isPending || data.shells < badge.price}
                onClick={() => buy.mutate({ kind: "badges", id: badge.id })}
              >
                {t("Buy · {price} shells", { price: badge.price })}
              </PixelButton>
            ) : (
              <span className="muted">{t("Locked")}</span>
            )}
          </article>
        ))}
      </div>
    </Panel>
  );
}
