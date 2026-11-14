import { useNavigate } from "react-router";
import { PageHead } from "../../../../components/PageHead";
import { PixelButton } from "../../../../components/PixelButton";
import { t } from "../../../../reef/i18n";
import { tierLabel, type HouseOverview, type HouseTarget } from "../../api/house";

type Props = {
  title: string;
  house: Pick<HouseOverview, "scope" | "house" | "rooms"> | undefined;
  targetKind: HouseTarget["kind"];
  visitId: string;
  updateVisitId: (id: string) => void;
};

export function Header({ title, house, targetKind, visitId, updateVisitId }: Props) {
  const navigate = useNavigate();
  return (
    <PageHead
      title={title}
      lead={
        house
          ? `${house.house.name} · ${t("{tier} · {rooms} rooms · {likes} likes", {
              tier: tierLabel(house.house.tier, house.house.tier_name),
              rooms: house.rooms.length,
              likes: house.house.likes,
            })}`
          : targetKind === "own"
            ? t("Your rooms, as they stand.")
            : t("Somebody else's rooms, as they stand.")
      }
      action={
        house?.scope === "own" ? (
          <form
            className="house-visit"
            onSubmit={(event) => {
              event.preventDefault();
              if (/^\d{15,21}$/.test(visitId.trim())) navigate(`/house/${visitId.trim()}`);
            }}
          >
            <input
              value={visitId}
              onChange={(event) => updateVisitId(event.target.value)}
              inputMode="numeric"
              placeholder={t("Player ID to visit")}
              aria-label={t("Discord ID of a player to visit")}
            />
            <PixelButton type="submit" variant="secondary" disabled={!/^\d{15,21}$/.test(visitId.trim())}>
              {t("Visit")}
            </PixelButton>
          </form>
        ) : (
          <PixelButton variant="secondary" onClick={() => navigate("/me/house")}>
            {t("My house")}
          </PixelButton>
        )
      }
    />
  );
}
