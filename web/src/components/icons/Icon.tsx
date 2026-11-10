import { iconNames, type IconName } from "./map";

export function Icon({ name, className = "" }: { name: IconName; className?: string }) {
  return (
    <svg className={`pixel-icon ${className}`} aria-hidden="true" focusable="false">
      <use href={`#px-${iconNames[name]}`} />
    </svg>
  );
}
