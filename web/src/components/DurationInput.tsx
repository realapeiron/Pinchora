import { useState } from "react";

import { UNITS, type Unit, joinSpan, splitSpan } from "../lib/roles";
import { translate } from "../i18n";

/**
 * A time as an amount and a unit (minutes, hours, days), reported in seconds; 0 is none. It starts from
 * `value` and keeps its own amount and unit while it is on screen: give it a new `key` to start it over.
 */
export function DurationInput({
  id,
  label,
  value,
  onChange,
  invalid,
}: {
  id: string;
  /** What the time is for, so each unit menu has a name of its own ("Wait before giving it"). */
  label: string;
  value: number;
  onChange: (secs: number) => void;
  invalid?: boolean;
}) {
  const [start] = useState(() => splitSpan(value));
  const [amount, setAmount] = useState(String(start.amount));
  const [unit, setUnit] = useState<Unit>(start.unit);
  const report = (nextAmount: string, nextUnit: Unit) => {
    const n = Number(nextAmount);
    onChange(nextAmount === "" || Number.isNaN(n) ? 0 : joinSpan(n, nextUnit));
  };
  return (
    <div className="duration">
      <input
        id={id}
        className="pixel-input"
        type="number"
        min={0}
        inputMode="numeric"
        value={amount}
        aria-describedby={`${id}-hint`}
        aria-invalid={invalid ? true : undefined}
        onChange={(event) => {
          setAmount(event.target.value);
          report(event.target.value, unit);
        }}
      />
      <select
        className="pixel-input pixel-select"
        aria-label={translate("Unit of time for {label}", { label })}
        value={unit}
        onChange={(event) => {
          const next = event.target.value as Unit;
          setUnit(next);
          report(amount, next);
        }}
      >
        {UNITS.map((u) => (
          <option key={u.unit} value={u.unit}>
            {translate(u.name)}
          </option>
        ))}
      </select>
    </div>
  );
}
