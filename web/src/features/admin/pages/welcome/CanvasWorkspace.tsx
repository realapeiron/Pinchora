import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../../i18n";
import { PixelButton } from "../../../../components/PixelButton";
import { type WelcomeModel } from "../../hooks/useWelcomeEditor";
import { CardCanvas } from "../../components/card/CardCanvas";

export function CanvasWorkspace({
  zoomBy,
  scale,
  setZoom,
  zoom,
  snapping,
  setSnapping,
  safeArea,
  setSafeArea,
  design,
  viewport,
  selected,
  fill,
  me,
  uploadUrls,
  atlas,
  setSelected,
  onLive,
  onCommit,
}: Pick<
  WelcomeModel,
  | "zoomBy"
  | "scale"
  | "setZoom"
  | "zoom"
  | "snapping"
  | "setSnapping"
  | "safeArea"
  | "setSafeArea"
  | "design"
  | "viewport"
  | "selected"
  | "fill"
  | "me"
  | "uploadUrls"
  | "atlas"
  | "setSelected"
  | "onLive"
  | "onCommit"
>) {
  return (
    <div className="card-center">
      <div className="card-zoom" role="group" aria-label={spaTranslate("Zoom")}>
        <PixelButton
          variant="ghost"
          className="button-small"
          onClick={() => zoomBy(-1)}
          aria-label={spaTranslate("Zoom out")}
        >
          −
        </PixelButton>
        <span className="pixel-label card-zoom-value">{Math.round(scale * 100)}%</span>
        <PixelButton
          variant="ghost"
          className="button-small"
          onClick={() => zoomBy(1)}
          aria-label={spaTranslate("Zoom in")}
        >
          +
        </PixelButton>
        <PixelButton
          variant="ghost"
          className="button-small"
          onClick={() => setZoom("fit")}
          aria-pressed={zoom === "fit"}
        >
          {" "}
          {spaTranslate("Fit")}{" "}
        </PixelButton>
        <label className="card-snap">
          <input type="checkbox" checked={snapping} onChange={(event) => setSnapping(event.target.checked)} />{" "}
          {spaTranslate("Snap")}{" "}
        </label>
        <label className="card-snap">
          <input type="checkbox" checked={safeArea} onChange={(event) => setSafeArea(event.target.checked)} />{" "}
          {deferredText("Safe area")}
        </label>
        <span className="muted fine card-size">
          {design.width}×{design.height}
        </span>
      </div>
      <div className="card-viewport" ref={viewport}>
        <CardCanvas
          design={design}
          zoom={scale}
          selected={selected}
          snapping={snapping}
          safeArea={safeArea}
          fill={fill}
          avatarUrl={me.user.avatar_url}
          uploads={uploadUrls}
          atlas={atlas}
          onSelect={setSelected}
          onLive={onLive}
          onCommit={onCommit}
        />
      </div>
    </div>
  );
}
