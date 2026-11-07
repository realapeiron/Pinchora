import { translate as spaTranslate } from "../i18n";
import { useEffect, useState } from "react";

import { ApiError, apiImage } from "../lib/api";
import { useDebounced } from "../lib/useDebounced";

/**
 * A PNG the bot draws, shown as it would appear in Discord. `body` is what to draw; it is sent a
 * moment after it stops changing, and the last picture stays up until the next one is ready.
 */
export function PreviewFrame({
  path,
  body,
  alt,
  width,
  height,
  delay = 300,
}: {
  path: string;
  body: object | null;
  alt: string;
  width: number;
  height: number;
  /** How long `body` must stay the same before it is sent, in ms. */
  delay?: number;
}) {
  const key = JSON.stringify(body);
  const settled = useDebounced(key, delay);
  const [url, setUrl] = useState<string | null>(null);
  const [state, setState] = useState<"loading" | "ready" | "error">("loading");
  const [message, setMessage] = useState("");
  // Bumped when a refused preview (too many asked for, the bot busy) is asked again by itself.
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (settled === "null") return;
    const controller = new AbortController();
    let made: string | null = null;
    let retry: number | undefined;
    setState("loading");
    apiImage(path, JSON.parse(settled), controller.signal)
      .then((blob) => {
        made = URL.createObjectURL(blob);
        setUrl(made);
        setState("ready");
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted) return;
        const wait =
          error instanceof ApiError && (error.status === 429 || error.status === 503)
            ? error.retryAfter
            : undefined;
        if (wait) retry = window.setTimeout(() => setAttempt((n) => n + 1), wait * 1000 + 250);
        const base = error instanceof Error ? error.message : spaTranslate("The preview could not be drawn.");
        setMessage(wait ? `${base} Trying again in ${wait} s.` : base);
        setState("error");
      });
    return () => {
      controller.abort();
      window.clearTimeout(retry);
      if (made) URL.revokeObjectURL(made);
    };
  }, [path, settled, attempt]);

  // Nothing to draw while the form has a problem: the last card stays, marked as out of date.
  const paused = key === "null";
  const busy = !paused && (state === "loading" || settled !== key);
  return (
    <figure className={`preview-frame ${busy || paused ? "is-busy" : ""}`} aria-busy={busy}>
      {url ? (
        <img src={url} alt={alt} width={width} height={height} className="preview-image" />
      ) : (
        <div className="preview-empty" style={{ aspectRatio: `${width} / ${height}` }} />
      )}
      {busy && <span className="preview-badge pixel-label">{spaTranslate("DRAWING…")}</span>}
      {paused && (
        <span className="preview-badge preview-badge-paused pixel-label">
          {spaTranslate("FIX THE FORM TO REDRAW")}
        </span>
      )}
      {state === "error" && !busy && <figcaption className="field-error">{message}</figcaption>}
    </figure>
  );
}
