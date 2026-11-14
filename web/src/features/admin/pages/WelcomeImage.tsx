import { Panel } from "../../../components/Panel";
import { useOutletContext } from "react-router";
import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { type Me, type Overview } from "../../../lib/api";
import { type Cards } from "../../../lib/card";
import { type AtlasIndex } from "../../../components/card/CardSvg";
import { useAtlasIndex, useCards, useMe } from "../../../lib/queries";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import { SaveBar } from "../../../components/SaveBar";
import { Toast } from "../../../components/Toast";
import { useWelcomeEditor } from "../hooks/useWelcomeEditor";
import { WelcomeToolbar } from "./welcome/WelcomeToolbar";
import { CanvasWorkspace } from "./welcome/CanvasWorkspace";
import { LayerList } from "./welcome/LayerList";
import { PropertyPanel } from "./welcome/PropertyPanel";
import { ImageAccessibility } from "./welcome/ImageAccessibility";
import { SavedVersions } from "./welcome/SavedVersions";
import { TryIt } from "./welcome/TryIt";
import { WelcomeDialogs } from "./welcome/WelcomeDialogs";
import "../../../styles/card-editor.css";

export function WelcomeImage() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const cards = useCards(guild);
  const atlas = useAtlasIndex();
  const me = useMe();
  if (cards.isPending || me.isPending) return <Loading label={deferredText("Opening the editor")} />;
  if (cards.isError) return <Problem error={cards.error} />;
  if (me.isError) return <Problem error={me.error} />;
  return (
    <Editor
      key={guild}
      guild={guild}
      server={data.guild.name}
      cards={cards.data}
      me={me.data}
      atlas={atlas.data}
    />
  );
}

function Editor({
  guild,
  server,
  cards,
  me,
  atlas,
}: {
  guild: string;
  server: string;
  cards: Cards;
  me: Me;
  atlas: AtlasIndex | undefined;
}) {
  const model = useWelcomeEditor({ guild, server, cards, me, atlas });
  const { imageOff, kind, simple, errors, local, changed, saving, save, reset, toast, clearToast } = model;
  return (
    <SettingsPage
      className="settings-page card-editor"
      title={deferredText("WELCOME IMAGE")}
      lead={deferredText(
        "Design the picture new members get, and the one that says goodbye. Drag things around on the canvas; Try it, further down, shows the picture the bot really draws.",
      )}
    >
      <WelcomeToolbar
        kind={model.kind}
        setKind={model.setKind}
        setSelected={model.setSelected}
        draft={model.draft}
        saved={model.saved}
        side={model.side}
        setSide={model.setSide}
        cards={model.cards}
        setDialog={model.setDialog}
        setHist={model.setHist}
        hist={model.hist}
      />
      {imageOff && (
        <p className="notice">
          {" "}
          {spaTranslate("This")} {kind}{" "}
          {deferredText(
            "goes out as text only, so the image is not sent. Pick another way to send it to use the design.",
          )}{" "}
        </p>
      )}

      <div className={`card-workspace ${simple ? "is-simple" : ""}`}>
        <CanvasWorkspace
          zoomBy={model.zoomBy}
          scale={model.scale}
          setZoom={model.setZoom}
          zoom={model.zoom}
          snapping={model.snapping}
          setSnapping={model.setSnapping}
          safeArea={model.safeArea}
          setSafeArea={model.setSafeArea}
          design={model.design}
          viewport={model.viewport}
          selected={model.selected}
          fill={model.fill}
          me={model.me}
          uploadUrls={model.uploadUrls}
          atlas={model.atlas}
          setSelected={model.setSelected}
          onLive={model.onLive}
          onCommit={model.onCommit}
        />
        <div className="card-aside">
          <LayerList
            design={model.design}
            share={model.share}
            ordered={model.ordered}
            selected={model.selected}
            setLayer={model.setLayer}
            setSelected={model.setSelected}
            restack={model.restack}
            layer={model.layer}
            duplicate={model.duplicate}
            removeLayer={model.removeLayer}
            addLayer={model.addLayer}
            setDialog={model.setDialog}
          />
          <PropertyPanel
            layer={model.layer}
            design={model.design}
            setLayer={model.setLayer}
            setDialog={model.setDialog}
            simple={model.simple}
            setDesign={model.setDesign}
            atlas={model.atlas}
            uploads={model.uploads}
            upload={model.upload}
            uploading={model.uploading}
          />
        </div>
      </div>

      <ImageAccessibility design={model.design} setDesign={model.setDesign} />
      <SavedVersions
        revisions={model.revisions}
        guild={model.guild}
        kind={model.kind}
        saved={model.saved}
        setSaved={model.setSaved}
        setHist={model.setHist}
        refreshRevisions={model.refreshRevisions}
        setToast={model.setToast}
        setErrors={model.setErrors}
      />
      {(errors.length > 0 || local.length > 0) && (
        <Panel title={deferredText("FIX THIS FIRST")} tone="accent">
          <ul className="card-errors" role="alert">
            {[...local, ...errors].map((e) => (
              <li key={e}>{e}</li>
            ))}
          </ul>
        </Panel>
      )}
      <TryIt
        showBot={model.showBot}
        setShowBot={model.setShowBot}
        discord={model.discord}
        testChannel={model.testChannel}
        setTestChannel={model.setTestChannel}
        sendTest={model.sendTest}
        testing={model.testing}
        local={model.local}
        guild={model.guild}
        design={model.design}
        kind={model.kind}
        side={model.side}
      />
      <WelcomeDialogs
        dialog={model.dialog}
        setDialog={model.setDialog}
        cards={model.cards}
        setDesign={model.setDesign}
        kind={model.kind}
        setSelected={model.setSelected}
        fill={model.fill}
        me={model.me}
        uploadUrls={model.uploadUrls}
        atlas={model.atlas}
        layer={model.layer}
        setLayer={model.setLayer}
        addLayer={model.addLayer}
        design={model.design}
      />
      <SaveBar
        count={changed}
        saving={saving}
        blocked={local.length > 0}
        error={local.length > 0 ? deferredText("Fix the marked layers first") : null}
        onSave={save}
        onReset={reset}
      />
      <Toast message={toast} onDone={clearToast} />
    </SettingsPage>
  );
}
