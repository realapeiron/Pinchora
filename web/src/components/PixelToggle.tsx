/** An on/off switch in the pixel style; mint when on. */
export function PixelToggle({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className={`pixel-toggle ${checked ? "is-on" : "is-off"}`}
      onClick={() => onChange(!checked)}
    >
      <span className="pixel-toggle-knob" />
      <span className="pixel-toggle-text" aria-hidden="true">
        {checked ? "ON" : "OFF"}
      </span>
    </button>
  );
}
