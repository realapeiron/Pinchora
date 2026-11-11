import { AtlasThumb } from "../../../components/card/CardProperties";
import { useAtlasIndex } from "../../../lib/queries";
import type { AchievementDraft } from "../api/achievements";
import { SYMBOLS } from "./achievementLabels";

export function Icon({ icon, atlas }: { icon: string; atlas: ReturnType<typeof useAtlasIndex>["data"] }) {
  return icon.startsWith("b_") ? (
    <AtlasThumb atlas={atlas} id={icon} size={44} />
  ) : (
    <span aria-hidden="true">{SYMBOLS[icon] ?? "★"}</span>
  );
}
export function AchievementPreview({
  item,
  t,
  atlas,
  earned,
  number,
}: {
  item: AchievementDraft;
  t: (key: string) => string;
  atlas: ReturnType<typeof useAtlasIndex>["data"];
  earned: boolean;
  number: (value: number) => string;
}) {
  return (
    <div className={`ach-preview ${earned ? "ach-preview-earned" : ""}`}>
      <div className="ach-preview-icon">
        <Icon icon={item.icon} atlas={atlas} />
      </div>
      <span className="pixel-label">{earned ? t("Earned") : t("Locked")}</span>
      <h3>{item.name || t("Name")}</h3>
      <p>{item.description || t("Description")}</p>
      <div className="ach-preview-track">
        <span style={{ width: earned ? "100%" : "35%" }} />
      </div>
      <small>
        {number(earned ? item.target : Math.max(0, Math.round(item.target * 0.35)))} / {number(item.target)}
      </small>
    </div>
  );
}
