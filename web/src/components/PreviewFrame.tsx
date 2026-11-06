import { useEffect, useState } from "react";

import { apiImage } from "../lib/api";
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

  useEffect(() => {
    if (settled === "null") return;
    const controller = new AbortController();
    let made: string | null = null;
    setState("loading");
    apiImage(path, JSON.parse(settled), controller.signal)
      .then((blob) => {
        made = URL.createObjectURL(blob);
        setUrl(made);
        setState("ready");
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted) return;
        setMessage(error instanceof Error ? error.message : "The preview could not be drawn.");
        setState("error");
      });
    return () => {
      controller.abort();
      if (made) URL.revokeObjectURL(made);
    };
  }, [path, settled]);

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
      {busy && <span className="preview-badge pixel-label">DRAWING…</span>}
      {paused && (
        <span className="preview-badge preview-badge-paused pixel-label">FIX THE FORM TO REDRAW</span>
      )}
      {state === "error" && !busy && <figcaption className="field-error">{message}</figcaption>}
    </figure>
  );
}
