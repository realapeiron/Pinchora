import { SettingsRow } from "../../../components/SettingsRow";

export function AntiNukeFlagRow({
  title,
  description,
  summary,
  enabled,
  disabled,
  onToggle,
}: {
  title: string;
  description: string;
  summary: string;
  enabled: boolean;
  disabled: boolean;
  onToggle: (value: boolean) => void;
}) {
  return (
    <SettingsRow
      title={title}
      description={description}
      summary={summary}
      enabled={enabled}
      toggleDisabled={disabled}
      onToggle={onToggle}
    >
      <p className="muted">{description}</p>
    </SettingsRow>
  );
}
