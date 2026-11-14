import type { ReactNode } from "react";
import type { Choice } from "../../../api/webPlayer";
import { t } from "../../../../../i18n";

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="player-field">
      <span>{label}</span>
      {children}
    </label>
  );
}
export function Picker({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string | null;
  options: Choice[];
  onChange: (value: string | null) => void;
}) {
  return (
    <Field label={label}>
      <select value={value ?? ""} onChange={(e) => onChange(e.target.value || null)}>
        <option value="">{t("None")}</option>
        {options.map((o) => (
          <option key={o.id} value={o.id}>
            {o.name}
          </option>
        ))}
      </select>
    </Field>
  );
}
