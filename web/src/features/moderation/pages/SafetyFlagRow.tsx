import { SettingsRow } from "../../../components/SettingsRow";

export function SafetyFlagRow({
  rowId,
  title,
  description,
  summary,
  enabled,
  onToggle,
}: {
  rowId: string;
  title: string;
  description: string;
  summary: string;
  enabled: boolean;
  onToggle: (value: boolean) => void;
}) {
  return (
    <SettingsRow
      rowId={rowId}
      title={title}
      description={description}
      summary={summary}
      enabled={enabled}
      onToggle={onToggle}
    >
      <p className="muted">{description}</p>
    </SettingsRow>
  );
}
