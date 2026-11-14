import { useState } from "react";
import { translate as t } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { Field } from "../../../../components/Field";
import { PixelButton } from "../../../../components/PixelButton";
import type { QueueRow, useModerationDesk } from "../../api/moderationDesk";

type ReviewSelection = { queue: string; row: QueueRow; accept: boolean };

type Props = {
  guild: string;
  reports: QueueRow[];
  appeals: QueueRow[];
  review: ReturnType<typeof useModerationDesk>["review"];
};

export function ReviewDesk({ guild, reports, appeals, review }: Props) {
  const [reason, setReason] = useState("");
  const [selected, setSelected] = useState<ReviewSelection | null>(null);
  return (
    <>
      {(
        [
          ["reports", reports, "Member reports"],
          ["appeals", appeals, "Member appeals"],
        ] as const
      ).map(([queue, rows, title]) => (
        <Panel key={queue} title={t(title)}>
          {!rows.length && <p className="empty-row">{t("The review queue is clear.")}</p>}
          <ul className="list">
            {rows.map((row) => (
              <li key={row.id} className="list-row">
                <div>
                  <strong>
                    #{row.id} · {row.user_id}
                  </strong>
                  <p>{row.body}</p>
                  {row.case_number && (
                    <p>
                      {t("Cases")} #{row.case_number}
                    </p>
                  )}
                  {row.reporter_id && (
                    <p>
                      {t("Reported by")}: {row.reporter_id}
                    </p>
                  )}
                  {row.channel_id && row.message_id && (
                    <p>
                      <a
                        href={`https://discord.com/channels/${guild}/${row.channel_id}/${row.message_id}`}
                        target="_blank"
                        rel="noreferrer"
                      >
                        {t("Open reported message")}
                      </a>
                    </p>
                  )}
                  <span>{t(row.status)}</span>
                </div>
                {row.status === "pending" && (
                  <div>
                    <PixelButton
                      variant="secondary"
                      onClick={() => {
                        setSelected({ queue, row, accept: true });
                        setReason("");
                      }}
                    >
                      {t("Accept")}
                    </PixelButton>{" "}
                    <PixelButton
                      variant="ghost"
                      onClick={() => {
                        setSelected({ queue, row, accept: false });
                        setReason("");
                      }}
                    >
                      {t("Reject")}
                    </PixelButton>
                  </div>
                )}
              </li>
            ))}
          </ul>
        </Panel>
      ))}
      {selected && (
        <Panel title={t("Confirm review")}>
          <p>
            #{selected.row.id} · {selected.row.user_id}
          </p>
          <p>
            {selected.queue === "reports"
              ? t("Record the outcome of this report for staff.")
              : selected.accept
                ? t("Accepting an appeal reverses the active ban or warn.")
                : t("The member will see this decision.")}
          </p>
          <Field label={t("Decision reason")} htmlFor="review-reason">
            <textarea
              /* eslint-disable-next-line jsx-a11y/no-autofocus -- The decision dialog focuses its reason field. */
              autoFocus
              id="review-reason"
              className="pixel-input"
              maxLength={512}
              value={reason}
              onChange={(e) => setReason(e.target.value)}
            />
          </Field>
          <PixelButton
            disabled={!reason.trim() || review.isPending}
            onClick={() =>
              review.mutate(
                { queue: selected.queue, id: selected.row.id, accept: selected.accept, reason },
                { onSuccess: () => setSelected(null) },
              )
            }
          >
            {t("Confirm decision")}
          </PixelButton>{" "}
          <PixelButton variant="ghost" onClick={() => setSelected(null)}>
            {t("Cancel")}
          </PixelButton>
        </Panel>
      )}
    </>
  );
}
