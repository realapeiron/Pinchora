import { useState } from "react";
import { translate as t } from "../../../../i18n";
import { useDiscord } from "../../../../lib/queries";
import { useModerationDesk, policyChanges, type ModPolicy, type QueueRow } from "../../api/moderationDesk";
import { PixelButton } from "../../../../components/PixelButton";
import { SaveBar } from "../../../../components/SaveBar";
import { Problem } from "../../../../components/States";
import { DeskBasics } from "./DeskBasics";
import { CleanupRules } from "./CleanupRules";
import { ConditionalRoles } from "./ConditionalRoles";
import { ReviewDesk } from "./ReviewDesk";

export function Desk({
  guild,
  saved,
  reports,
  appeals,
}: {
  guild: string;
  saved: ModPolicy;
  reports: QueueRow[];
  appeals: QueueRow[];
}) {
  const [draft, setDraft] = useState(saved);
  const { save, preview, review } = useModerationDesk(guild);
  const discord = useDiscord(guild);
  const change = <K extends keyof ModPolicy>(key: K, value: ModPolicy[K]) =>
    setDraft({ ...draft, [key]: value });
  const error = save.error ?? preview.error ?? review.error;
  return (
    <>
      <DeskBasics draft={draft} change={change} info={discord.data} />
      <CleanupRules draft={draft} change={change} info={discord.data} />
      <ConditionalRoles draft={draft} change={change} info={discord.data} />
      <div className="add-actions">
        <PixelButton variant="secondary" disabled={preview.isPending} onClick={() => preview.mutate(draft)}>
          {t("Validate policy")}
        </PixelButton>
      </div>
      {error && <Problem error={error} />}
      <SaveBar
        count={policyChanges(saved, draft)}
        saving={save.isPending}
        error={save.isError ? save.error.message : null}
        onSave={() => save.mutate(draft)}
        onReset={() => {
          setDraft(saved);
          save.reset();
        }}
      />
      {save.isSuccess && <p role="status">{t("Policy saved.")}</p>}
      {preview.isSuccess && <p role="status">{t("Policy checks passed.")}</p>}
      <ReviewDesk guild={guild} reports={reports} appeals={appeals} review={review} />
    </>
  );
}
