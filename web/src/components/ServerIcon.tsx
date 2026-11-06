import { initials } from "../lib/text";

const TILES = ["tile-accent", "tile-gold", "tile-mint", "tile-heart"];

/** A server's icon, or its initials on a coloured tile. */
export function ServerIcon({
  id,
  name,
  url,
  size = 48,
}: {
  id: string;
  name: string;
  url: string | null;
  size?: number;
}) {
  if (url) {
    return <img className="server-icon" src={url} alt="" width={size} height={size} />;
  }
  const tile = TILES[Number(id.slice(-2)) % TILES.length] ?? TILES[0];
  return (
    <span
      className={`server-icon server-icon-initials ${tile}`}
      aria-hidden="true"
      style={{ width: size, height: size, fontSize: size >= 48 ? 16 : 12 }}
    >
      {initials(name)}
    </span>
  );
}
