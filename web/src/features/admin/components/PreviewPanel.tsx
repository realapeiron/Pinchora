import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { useMemo, useState } from "react";

import { ApiError } from "../../../lib/api";
import { type TagKind, useTagPreview } from "../../../lib/queries";
import { useDebounced } from "../../../lib/useDebounced";
import { Field } from "../../../components/Field";
import { MessagePreview } from "../../../components/MessagePreview";
import { Panel } from "../../../components/Panel";
import { PixelButton } from "../../../components/PixelButton";

/**
 * The message a draft would send, drawn by the bot's own code (nothing is sent or saved). A command also
 * has a box for what a member would type after its name.
 */
export function PreviewPanel({
  guild,
  kind,
  body,
  args,
  onArgs,
  ready = true,
}: {
  guild: string;
  kind: TagKind;
  /** What the preview endpoint takes: the draft (and the arguments for a command). */
  body: unknown;
  args?: string;
  onArgs?: (next: string) => void;
  /** Whether the draft says enough to ask about; before that the panel says what is missing. */
  ready?: boolean;
}) {
  // The draft is a new object on every render; its text says whether anything really changed.
  const key = JSON.stringify(body);
  const settledKey = useDebounced(key, 600);
  const settled = useMemo(() => JSON.parse(settledKey) as unknown, [settledKey]);
  const [roll, setRoll] = useState(0);
  const preview = useTagPreview(guild, kind, settled, ready, roll);
  const stale = settledKey !== key || preview.isFetching;
  return (
    <Panel
      title={spaTranslate("PREVIEW")}
      aside={
        <PixelButton variant="ghost" className="button-small" onClick={() => setRoll(roll + 1)}>
          {" "}
          {deferredText("Roll again")}{" "}
        </PixelButton>
      }
    >
      {onArgs !== undefined && (
        <Field
          label={deferredText("Try it with")}
          htmlFor="preview-args"
          hint={deferredText("What a member would type after the command's name.")}
        >
          <input
            id="preview-args"
            className="pixel-input"
            value={args ?? ""}
            maxLength={200}
            placeholder={deferredText("Sam looks great")}
            onChange={(event) => onArgs(event.target.value)}
          />
        </Field>
      )}
      <div className={`preview-live ${stale ? "is-busy" : ""}`} aria-live="polite" aria-busy={stale}>
        {!ready ? (
          <p className="muted">{deferredText("Write a reply to see it here.")}</p>
        ) : preview.data ? (
          <MessagePreview preview={preview.data} />
        ) : preview.isError ? (
          <p className="muted">
            {preview.error instanceof ApiError && preview.error.status === 422
              ? deferredText("Fix this first to see it: {arg1}", { arg1: preview.error.message })
              : deferredText("The preview is not available just now.")}
          </p>
        ) : (
          <p className="muted">…</p>
        )}
        {preview.data && preview.isError && (
          <p className="field-hint muted">
            {preview.error instanceof ApiError
              ? preview.error.message
              : deferredText("The preview is not up to date.")}
          </p>
        )}
      </div>
      {preview.data && preview.data.notes.length > 0 && (
        <ul className="preview-notes muted">
          {preview.data.notes.map((note) => (
            <li key={note}>{note}</li>
          ))}
        </ul>
      )}
    </Panel>
  );
}
