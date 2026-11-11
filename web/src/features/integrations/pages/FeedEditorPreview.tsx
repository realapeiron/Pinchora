import { Panel } from "../../../components/Panel";
import { MessagePreview } from "../../../components/MessagePreview";
import { PixelButton } from "../../../components/PixelButton";
import type { useFeedEditor } from "./useFeedEditor";

export function FeedEditorPreview({ model }: { model: ReturnType<typeof useFeedEditor> }) {
  const { t, feed, preview, runTest, test, changes, tested } = model;
  return (
    <aside className="editor-rail settings-detail-preview">
      <Panel title={t("preview")}>
        <div className={`preview-live ${preview.isFetching ? "is-busy" : ""}`} aria-live="polite">
          {preview.data ? (
            <MessagePreview preview={preview.data} label={t("feed-post-preview")} />
          ) : preview.isError ? (
            <p className="muted">{preview.error.message}</p>
          ) : (
            <p className="muted">…</p>
          )}
        </div>
        {preview.data && preview.data.notes.length > 0 && (
          <ul className="preview-notes muted">
            {preview.data.notes.map((note) => (
              <li key={note}>{note}</li>
            ))}
          </ul>
        )}
        {feed && (
          <div className="feed-test">
            <PixelButton
              variant="secondary"
              onClick={runTest}
              disabled={test.isPending || changes > 0}
              title={changes > 0 ? t("save-first-the-test-uses-the-saved-feed") : undefined}
            >
              {test.isPending ? t("posting") : t("send-a-test-post")}
            </PixelButton>
            <p className="field-hint muted">
              {changes > 0
                ? t("save-first-the-test-uses-the-saved-feed")
                : t("posts-the-latest-item-in-the-channel-now")}
            </p>
            {tested && changes === 0 && (
              <p className={tested.ok ? "feed-test-ok" : "field-error"} role={tested.ok ? "status" : "alert"}>
                {tested.text}
              </p>
            )}
          </div>
        )}
      </Panel>
    </aside>
  );
}
