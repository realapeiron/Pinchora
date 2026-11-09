import { useEffect, useRef } from "react";
import { t, useLocale } from "../../../i18n";
import type { Art } from "../../../iso/art";
import "../../../styles/reef-loop.css";

export interface ReefHome {
  name: string;
  fish: { slot: number; id: string; name: string; name_tr: string; size_cm: number }[];
  garden: { seaweed: number; planted_at: string | null; watered_at: string | null } | null;
}
function Fish({ art, id }: { art?: Art; id: string }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const sprite = art?.atlas.get(`c_${id}`);
  useEffect(() => {
    if (sprite)
      ref.current
        ?.getContext("2d")
        ?.putImageData(new ImageData(sprite.data, sprite.width, sprite.height), 0, 0);
  }, [sprite]);
  return sprite ? (
    <canvas
      ref={ref}
      width={sprite.width}
      height={sprite.height}
      style={{ width: sprite.width * 3, height: sprite.height * 3 }}
      aria-hidden="true"
    />
  ) : null;
}
export function ReefLoop({ data, art, own }: { data: ReefHome; art?: Art; own: boolean }) {
  const lang = useLocale();
  const garden = data.garden;
  return (
    <section className="reef-home" aria-label={t("Aquarium and garden")}>
      <div className="reef-home-tank">
        <h2>{data.name}</h2>
        <p>{t("{count}/6 fish at home", { count: data.fish.length })}</p>
        <ul className="reef-home-fish">
          {data.fish.map((fish) => (
            <li key={fish.slot}>
              <Fish art={art} id={fish.id} />
              <span>{lang === "tr" ? fish.name_tr : fish.name}</span>
              <small>
                {t("{size} cm", {
                  size: new Intl.NumberFormat(lang, { maximumFractionDigits: 1 }).format(fish.size_cm),
                })}
              </small>
            </li>
          ))}
        </ul>
        {data.fish.length === 0 && <p>{t("Bring a catch home with /house aquarium in Discord.")}</p>}
        {own && <p className="muted">{t("Name your tank and move fish with /house aquarium.")}</p>}
      </div>
      {garden && (
        <div className="reef-home-garden">
          <h2>{t("Brinebud Garden")}</h2>
          <p>{t("Seaweed: {count}/999", { count: garden.seaweed })}</p>
          <p>
            {garden.planted_at
              ? garden.watered_at
                ? t("Watered. Your plants never wither.")
                : t("Growing. Water once after the first hour.")
              : t("One empty bed, ready for seeds.")}
          </p>
          {garden.planted_at && (
            <p>
              {t("Harvest from {time}", {
                time: new Date(new Date(garden.planted_at).getTime() + 8 * 3600_000).toLocaleString(lang),
              })}
            </p>
          )}
          <p>{t("Use /house garden to tend your plants, then /house craft to make furniture.")}</p>
        </div>
      )}
    </section>
  );
}
