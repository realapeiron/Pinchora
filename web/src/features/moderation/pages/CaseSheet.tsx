import { t as deferredText } from "../../../i18n/dashboard";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { dateTime } from "../../../i18n";
import { t } from "../../../i18n/dashboard";
import { Icon } from "../../../components/icons/Icon";
import { Field } from "../../../components/Field";
import { PixelButton } from "../../../components/PixelButton";
import { Loading, Problem } from "../../../components/States";
import { Toast } from "../../../components/Toast";
import { ApiError, type Case } from "../../../lib/api";
import { useCase, useModeration, useSaveReason } from "../../../lib/queries";
import { timeAgo } from "../../../lib/text";
import { trapTab } from "../../../components/overlayFocus";
import { CasePills } from "./CaseList";
import { nextWarningRung } from "./caseView";
import { CaseRevoke } from "./CaseRevoke";

export function CaseSheet({
  guild,
  number,
  onClose,
  onOpen,
}: {
  guild: string;
  number: number;
  onClose: () => void;
  onOpen: (number: number) => void;
}) {
  const detail = useCase(guild, number);
  const [revokeNotice, setRevokeNotice] = useState<string | null>(null);
  const settings = useModeration(guild).query;
  const dialog = useRef<HTMLDivElement>(null);
  const close = useRef<HTMLButtonElement>(null);
  const returnFocus = useRef<HTMLElement | null>(null);
  const closeAction = useRef(onClose);
  closeAction.current = onClose;
  useLayoutEffect(() => {
    returnFocus.current = document.activeElement as HTMLElement;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    close.current?.focus();
    const keydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        closeAction.current();
      } else if (dialog.current) trapTab(event, dialog.current);
    };
    document.addEventListener("keydown", keydown);
    return () => {
      document.removeEventListener("keydown", keydown);
      document.body.style.overflow = previousOverflow;
      if (returnFocus.current?.isConnected) returnFocus.current.focus();
    };
  }, []);
  useEffect(() => {
    close.current?.focus();
  }, [number]);
  const entry = detail.data?.case;
  const history = entry?.on_channel ? [] : (detail.data?.history ?? []);
  const hint = entry && settings.data ? nextWarningRung(entry, history, settings.data.ladder) : null;
  // Dashboard styles are scoped under .shell-page, so the portal keeps that wrapper around the backdrop.
  return createPortal(
    <div className="shell-page">
      <div className="cases-backdrop">
        <button
          type="button"
          className="cases-underlay"
          aria-label={t("Close overlay")}
          tabIndex={-1}
          onClick={onClose}
        />
        <div
          ref={dialog}
          role="dialog"
          aria-modal="true"
          aria-labelledby="case-sheet-title"
          className="cases-sheet"
        >
          <div className="cases-sheet-head">
            <div>
              {entry && (
                <div className="cases-sheet-pills">
                  <CasePills entry={entry} />
                </div>
              )}
              <h2 id="case-sheet-title">{t("Case #{number}", { number })}</h2>
            </div>
            <button
              ref={close}
              type="button"
              className="cases-sheet-close"
              aria-label={t("Close case")}
              onClick={onClose}
              onKeyDown={(event) => {
                if (event.key === "Escape") {
                  event.preventDefault();
                  event.stopPropagation();
                  onClose();
                }
              }}
            >
              <Icon name="x" />
            </button>
          </div>
          {detail.isPending && <Loading label={deferredText("Opening the case")} />}
          {detail.isError &&
            (detail.error instanceof ApiError && detail.error.status === 404 ? (
              <p role="alert">{t("Case #{number} does not exist.", { number })}</p>
            ) : (
              <Problem error={detail.error} />
            ))}
          {entry && (
            <>
              <dl className="cases-facts">
                <dt>{entry.on_channel ? t("Channel") : t("Member")}</dt>
                <dd>
                  {entry.target_name || entry.target_id} <small>({entry.target_id})</small>
                </dd>
                <dt>{deferredText("Moderator")}</dt>
                <dd>
                  {entry.moderator_name} <small>({entry.moderator_id})</small>
                </dd>
                <dt>{t("Duration")}</dt>
                <dd>
                  {entry.duration || "—"}
                  {entry.expires_at && (
                    <small>
                      {" "}
                      · {t("Ends")} {dateTime(entry.expires_at)}
                    </small>
                  )}
                </dd>
                <dt>{t("When")}</dt>
                <dd>
                  <time dateTime={entry.created_at}>
                    {timeAgo(entry.created_at)} · {dateTime(entry.created_at)}
                  </time>
                </dd>
                {entry.details && (
                  <>
                    <dt>{t("Details")}</dt>
                    <dd>{entry.details}</dd>
                  </>
                )}
                {entry.dm_sent !== null && (
                  <>
                    <dt>{t("Direct message")}</dt>
                    <dd>{entry.dm_sent ? deferredText("Delivered") : deferredText("Not delivered")}</dd>
                  </>
                )}
              </dl>
              {hint && (
                <p className="cases-ladder-hint">
                  {t(
                    hint.active === 1
                      ? "{member} has {count} active warning. Next: {action}."
                      : "{member} has {count} active warnings. Next: {action}.",
                    {
                      member: entry.target_name || entry.target_id,
                      count: hint.active,
                      action: hint.rung.label,
                    },
                  )}
                </p>
              )}
              <ReasonEditor key={entry.number} guild={guild} entry={entry} />
              {entry.active && !entry.on_channel && ["warn", "timeout", "ban"].includes(entry.kind) && (
                <CaseRevoke
                  key={`revoke-${entry.number}`}
                  guild={guild}
                  entry={entry}
                  onRevoked={() => {
                    setRevokeNotice(t("Case revoked."));
                    close.current?.focus();
                  }}
                />
              )}
              {!entry.on_channel && (
                <section className="cases-history" aria-label={deferredText("Their other cases")}>
                  <h3>{deferredText("Their other cases")}</h3>
                  {history.length ? (
                    <ol>
                      {history.slice(0, 5).map((item) => (
                        <li key={item.number}>
                          <button type="button" onClick={() => onOpen(item.number)}>
                            <strong>
                              #{item.number} · {t(item.label)}
                            </strong>
                            <span>{item.reason || "—"}</span>
                            <time dateTime={item.created_at}>{timeAgo(item.created_at)}</time>
                          </button>
                        </li>
                      ))}
                    </ol>
                  ) : (
                    <p className="muted">{t("No other cases for this member.")}</p>
                  )}
                </section>
              )}
            </>
          )}
          <Toast message={revokeNotice} onDone={() => setRevokeNotice(null)} />
        </div>
      </div>
    </div>,
    document.body,
  );
}

function ReasonEditor({ guild, entry }: { guild: string; entry: Case }) {
  const save = useSaveReason(guild);
  const [reason, setReason] = useState(entry.reason ?? "");
  const [notice, setNotice] = useState<string | null>(null);
  const changed = reason.trim() !== (entry.reason ?? "");
  const error =
    save.error instanceof ApiError
      ? save.error.message
      : save.error
        ? deferredText("Could not save the reason.")
        : null;
  return (
    <div className="cases-reason-editor">
      <Field
        label={t("Reason")}
        htmlFor={`case-reason-${entry.number}`}
        error={error}
        hint={deferredText("Saving edits the case in the mod-log too.")}
        aside={<span className={`counter ${reason.length > 512 ? "is-over" : ""}`}>{reason.length}/512</span>}
      >
        <textarea
          id={`case-reason-${entry.number}`}
          className="pixel-input pixel-textarea"
          rows={3}
          value={reason}
          onChange={(event) => setReason(event.target.value)}
        />
      </Field>
      <PixelButton
        variant="secondary"
        disabled={!changed || save.isPending || reason.length > 512}
        onClick={() =>
          save.mutate(
            { number: entry.number, reason: reason.trim() || null },
            {
              onSuccess: () => setNotice(t("Case reason saved.")),
            },
          )
        }
      >
        {save.isPending
          ? t("Saving…")
          : save.isSuccess && !changed
            ? t("Saved")
            : deferredText("Save reason")}
      </PixelButton>
      <Toast message={notice} onDone={() => setNotice(null)} />
    </div>
  );
}
