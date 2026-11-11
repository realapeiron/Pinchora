import { dateTime } from "../../../i18n";
import { MessagePreview } from "../../../components/MessagePreview";
import { PixelButton } from "../../../components/PixelButton";
import type { EmbedSpec } from "../../../lib/api";
import type { catalogWords } from "../api/catalogWords";

export function UtilityInlinePreview({
  item,
  runs,
  t,
  onClose,
}: {
  item: { content: string; embed: EmbedSpec | null };
  runs: string[];
  t: ReturnType<typeof catalogWords>;
  onClose: () => void;
}) {
  return (
    <div className="settings-detail-preview">
      <div className="utility-inline-preview-head">
        <strong>{t("PREVIEW")}</strong>
        <PixelButton variant="ghost" onClick={onClose}>
          {t("Close preview")}
        </PixelButton>
      </div>
      <MessagePreview preview={{ text: item.content, embed: item.embed, ephemeral: false, notes: [] }} />
      {runs.length > 0 && (
        <div className="utility-runs">
          <strong>{t("NEXT RUNS")}</strong>
          <ol>
            {runs.map((run) => (
              <li key={run}>{dateTime(run)}</li>
            ))}
          </ol>
        </div>
      )}
    </div>
  );
}
