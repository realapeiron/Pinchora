import { SHOTS, altOf } from "./shots";
import { translate } from "../i18n";
import type { ShotKey } from "./features";

/** A card the bot drew, in a pixel frame, with its size set so the page does not jump when it loads. */
export function Shot({
  name,
  className,
  eager = false,
  alt,
}: {
  name: ShotKey;
  className?: string;
  eager?: boolean;
  alt?: string;
}) {
  const shot = SHOTS[name];
  return (
    <div className={["panel", "shot", className].filter(Boolean).join(" ")}>
      <img
        src={shot.src}
        width={shot.width}
        height={shot.height}
        alt={alt ?? translate(altOf(name))}
        loading={eager ? "eager" : "lazy"}
        decoding={eager ? "sync" : "async"}
        fetchPriority={eager ? "high" : "auto"}
      />
    </div>
  );
}
