import { Panel } from "../../../../components/Panel";
import { t } from "../../../../reef/i18n";

/** What is in the room, by name, for anyone who cannot point at the picture. */
export function Contents({ drawn }: { drawn: { kind: string; label: string }[] }) {
  const counts = new Map<string, number>();
  for (const info of drawn) {
    const key =
      info.kind === "furniture"
        ? info.label
        : `${info.kind === "pet" ? t("Pet") : t("Character")}: ${info.label}`;
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  const rows = [...counts].sort(([a], [b]) => a.localeCompare(b));
  return (
    <Panel title={t("In this room")} aside={<span className="pixel-label muted">{drawn.length}</span>}>
      {rows.length === 0 ? (
        <p className="muted">{t("Nothing yet.")}</p>
      ) : (
        <ul className="house-contents">
          {rows.map(([name, n]) => (
            <li key={name}>
              <span>{name}</span>
              <span className="muted">×{n}</span>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}
