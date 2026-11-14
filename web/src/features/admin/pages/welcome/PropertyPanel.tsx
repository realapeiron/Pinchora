import { t as deferredText } from "../../../../i18n/dashboard";
import { Panel } from "../../../../components/Panel";
import { type WelcomeModel } from "../../hooks/useWelcomeEditor";
import { CanvasProperties, LayerProperties } from "../../../../components/card/CardProperties";

export function PropertyPanel({
  layer,
  design,
  setLayer,
  setDialog,
  simple,
  setDesign,
  atlas,
  uploads,
  upload,
  uploading,
}: Pick<
  WelcomeModel,
  | "layer"
  | "design"
  | "setLayer"
  | "setDialog"
  | "simple"
  | "setDesign"
  | "atlas"
  | "uploads"
  | "upload"
  | "uploading"
>) {
  return (
    <div className="card-props">
      <Panel title={deferredText("PROPERTIES")}>
        {layer ? (
          <LayerProperties
            layer={layer}
            canvas={design}
            onChange={setLayer}
            onPickSticker={() => setDialog("swap-sticker")}
            simple={simple}
          />
        ) : (
          <CanvasProperties
            design={design}
            onChange={setDesign}
            atlas={atlas}
            uploads={uploads}
            onUpload={upload}
            uploading={uploading}
            simple={simple}
          />
        )}
      </Panel>
    </div>
  );
}
