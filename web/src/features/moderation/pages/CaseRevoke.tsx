import { useEffect, useRef, useState } from "react";
import { translate as spaTranslate } from "../../../i18n";
import { Field } from "../../../components/Field";
import { t } from "../../../i18n/dashboard";
import { ApiError, type Case } from "../../../lib/api";
import { useRevokeCase } from "../../../lib/queries";

export function CaseRevoke({
  guild,
  entry,
  onRevoked,
}: {
  guild: string;
  entry: Case;
  onRevoked: () => void;
}) {
  const revoke = useRevokeCase(guild);
  const [confirming, setConfirming] = useState(false);
  const [reason, setReason] = useState("");
  const section = useRef<HTMLElement>(null);
  const start = useRef<HTMLButtonElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const hadConfirm = useRef(false);
  useEffect(() => {
    if (confirming) {
      hadConfirm.current = true;
      cancel.current?.focus();
    } else if (hadConfirm.current) start.current?.focus();
  }, [confirming]);
  useEffect(() => {
    if (!confirming) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && section.current?.contains(document.activeElement)) {
        event.preventDefault();
        event.stopImmediatePropagation();
        setConfirming(false);
      }
    };
    document.addEventListener("keydown", onKeyDown, true);
    return () => document.removeEventListener("keydown", onKeyDown, true);
  }, [confirming]);
  const kind = entry.kind === "warn" ? t("warning") : entry.kind === "timeout" ? t("timeout") : t("Ban case");
  const name = entry.target_name ? `@${entry.target_name.replace(/^@/, "")}` : `<@${entry.target_id}>`;
  const error =
    revoke.error instanceof ApiError
      ? revoke.error.message
      : revoke.error
        ? t("Could not revoke the case.")
        : null;
  return (
    <section ref={section} className="cases-revoke">
      {!confirming ? (
        <button
          ref={start}
          type="button"
          className="pixel-button pixel-button-secondary"
          onClick={() => setConfirming(true)}
        >
          {t("Revoke")}
        </button>
      ) : (
        <div className="cases-revoke-confirm" role="group" aria-label={t("Confirm revoke")}>
          <p>
            {entry.kind === "warn"
              ? t("Remove the warning for {member}? Their active warning count drops by one.", {
                  member: name,
                })
              : t("Remove the {kind} for {member}?", { kind, member: name })}
          </p>
          <Field
            label={t("Reason (optional)")}
            htmlFor={`revoke-reason-${entry.number}`}
            error={error}
            aside={
              <span className={`counter ${reason.length > 512 ? "is-over" : ""}`}>{reason.length}/512</span>
            }
          >
            <textarea
              id={`revoke-reason-${entry.number}`}
              className="pixel-input pixel-textarea"
              rows={2}
              value={reason}
              onChange={(event) => setReason(event.target.value)}
            />
          </Field>
          <div className="cases-revoke-actions">
            <button
              ref={cancel}
              type="button"
              className="pixel-button pixel-button-secondary"
              onClick={() => setConfirming(false)}
            >
              {spaTranslate("Cancel")}
            </button>
            <button
              type="button"
              className="pixel-button pixel-button-primary"
              disabled={revoke.isPending || reason.length > 512}
              onClick={() =>
                revoke.mutate(
                  {
                    number: entry.number,
                    reason: reason.trim() || null,
                  },
                  {
                    onSuccess: () => {
                      setConfirming(false);
                      onRevoked();
                    },
                  },
                )
              }
            >
              {revoke.isPending ? t("Revoking…") : t("Yes, revoke")}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
