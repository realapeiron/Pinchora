import { SettingsRow } from "../../../components/SettingsRow";

export function SafetyFlagRow({
  title,
  description,
  summary,
  enabled,
  onToggle,
}: {
  title: string;
  description: string;
  summary: string;
  enabled: boolean;
  onToggle: (value: boolean) => void;
}) {
  return (
    <SettingsRow
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
