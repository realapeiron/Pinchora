import { useContext } from "react";
import { Field } from "../../../components/Field";
import type { Recurrence } from "../api/utility";
import { UtilityWords } from "./utilityWords";

export function TimeFields({
  schedule,
  onChange,
}: {
  schedule: Extract<Recurrence, { kind: "daily" | "weekly" }>;
  onChange: (s: Recurrence) => void;
}) {
  const { t } = useContext(UtilityWords);
  return (
    <div className="utility-two">
      <Field label={t("Hour")} htmlFor="schedule-hour">
        <input
          id="schedule-hour"
          className="pixel-input"
          type="number"
          min={0}
          max={23}
          value={schedule.hour}
          onChange={(e) => onChange({ ...schedule, hour: Number(e.target.value) })}
        />
      </Field>
      <Field label={t("Minute")} htmlFor="schedule-minute">
        <input
          id="schedule-minute"
          className="pixel-input"
          type="number"
          min={0}
          max={59}
          value={schedule.minute}
          onChange={(e) => onChange({ ...schedule, minute: Number(e.target.value) })}
        />
      </Field>
    </div>
  );
}
