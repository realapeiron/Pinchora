/** One-of-several picker drawn as cards: a radio group with a title and a line per option. */
import { translate } from "../i18n";

export function Choice<T extends string>({
  name,
  value,
  options,
  onChange,
  disabled,
}: {
  name: string;
  value: T;
  options: { value: T; title: string; detail: string }[];
  onChange: (next: T) => void;
  disabled?: boolean;
}) {
  return (
    <div className="choice" role="radiogroup" aria-label={translate(name)}>
      {options.map((option) => {
        const checked = option.value === value;
        return (
          <label key={option.value} className={`choice-card ${checked ? "is-checked" : ""}`}>
            <input
              type="radio"
              name={name}
              value={option.value}
              checked={checked}
              disabled={disabled}
              onChange={() => onChange(option.value)}
            />
            <span className="choice-title">{translate(option.title)}</span>
            <span className="choice-detail muted">{translate(option.detail)}</span>
          </label>
        );
      })}
    </div>
  );
}
