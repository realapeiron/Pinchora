import { type ReactNode, useState } from "react";
import { t as deferredText } from "../../i18n/dashboard";
import { CARD, STICKER_GROUPS, stickerName } from "../../lib/card";
import { type AtlasIndex, GLYPHS } from "./CardSvg";
import { AtlasThumb } from "./PropertyControls";
import { translate as spaTranslate } from "../../i18n";
import { PixelButton } from "../PixelButton";

export { type Edit, NumberField, ColorField, AtlasThumb } from "./PropertyControls";
export { CanvasProperties } from "./CanvasProperties";
export { LayerProperties } from "./LayerProperties";

/** A modal panel with a close button; Escape closes it too. */
export function Dialog({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <div
      className="dialog-backdrop"
      role="presentation"
      onPointerDown={(event) => event.target === event.currentTarget && onClose()}
      onKeyDown={(event) => event.key === "Escape" && onClose()}
    >
      <div className="panel panel-gold dialog card-dialog" role="dialog" aria-modal="true" aria-label={title}>
        <div className="panel-body">
          <header className="panel-head">
            <h2 className="pixel-title">{title}</h2>
            {/* eslint-disable-next-line jsx-a11y/no-autofocus -- The dialog close control receives initial focus. */}
            <PixelButton variant="ghost" className="button-small" onClick={onClose} autoFocus>
              {spaTranslate("Close")}
            </PixelButton>
          </header>
          {children}
        </div>
      </div>
    </div>
  );
}

/** The sticker chooser: the pixel glyphs, then the atlas by group, with a search. */
export function StickerPicker({
  atlas,
  onPick,
  onClose,
}: {
  atlas: AtlasIndex | undefined;
  onPick: (sprite: { id: string; w: number; h: number }) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const names = Object.keys(atlas ?? {}).sort();
  const groups = [
    { name: deferredText("Pixel glyphs"), ids: CARD.glyphs },
    ...STICKER_GROUPS.map((g) => ({ name: g.name, ids: names.filter((n) => n.startsWith(g.prefix)) })),
  ];
  const q = query.trim().toLowerCase();
  const pick = (id: string) => {
    const glyph = GLYPHS[id];
    const region = atlas?.[id];
    if (glyph) onPick({ id, w: Math.max(...glyph.map((r) => r.length)), h: glyph.length });
    else if (region) onPick({ id, w: region.w, h: region.h });
  };
  return (
    <Dialog title={deferredText("CHOOSE A STICKER")} onClose={onClose}>
      <input
        className="pixel-input"
        placeholder={deferredText("Search, e.g. cat, koi, lantern")}
        aria-label={deferredText("Search stickers")}
        value={query}
        onChange={(event) => setQuery(event.target.value)}
      />
      <div className="sticker-groups">
        {groups.map((g) => {
          // Furniture is a long list: it shows when searched for.
          const ids = g.ids.filter((id) =>
            q ? stickerName(id).toLowerCase().includes(q) : g.name !== "Furniture",
          );
          if (ids.length === 0) return null;
          return (
            <section key={g.name}>
              <h4 className="pixel-label sticker-group">{spaTranslate(g.name).toLocaleUpperCase()}</h4>
              <div className="sticker-grid">
                {ids.slice(0, 120).map((id) => (
                  <button
                    key={id}
                    type="button"
                    className="sticker"
                    title={stickerName(id)}
                    aria-label={stickerName(id)}
                    onClick={() => pick(id)}
                  >
                    {GLYPHS[id] ? <GlyphThumb id={id} /> : <AtlasThumb atlas={atlas} id={id} size={44} />}
                  </button>
                ))}
              </div>
            </section>
          );
        })}
        {!q && <p className="fine muted">{deferredText("Search to find furniture too.")}</p>}
      </div>
    </Dialog>
  );
}

/** A glyph sticker drawn as its pixels. */
function GlyphThumb({ id }: { id: string }) {
  const rows = GLYPHS[id] ?? [];
  const cols = Math.max(...rows.map((r) => r.length));
  return (
    <svg
      className="atlas-thumb"
      width={cols * 5}
      height={rows.length * 5}
      viewBox={`0 0 ${cols} ${rows.length}`}
      aria-hidden="true"
      shapeRendering="crispEdges"
    >
      {rows.flatMap((row, r) =>
        [...row].map((ch, c) =>
          ch === "#" || ch === "o" ? (
            <rect
              key={`${r}-${c}`}
              x={c}
              y={r}
              width={1}
              height={1}
              fill={ch === "#" ? "#ffd166" : "#fff1e8"}
            />
          ) : null,
        ),
      )}
    </svg>
  );
}
