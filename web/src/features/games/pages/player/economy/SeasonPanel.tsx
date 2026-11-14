import { Panel } from "../../../../../components/Panel";
import { PixelButton } from "../../../../../components/PixelButton";
import { t, dateTime } from "../../../../../i18n";
import type { Economy } from "../../../../economy";
import type { Run } from "../../../../economy";

export function SeasonPanel({
  d,
  busy,
  run,
  xp,
  prestige,
  setPrestige,
}: {
  d: Economy;
  busy: boolean;
  run: Run;
  xp: number;
  prestige: boolean;
  setPrestige: (value: boolean) => void;
}) {
  const p = d.player;
  return (
    <>
      <Panel title={t("Free season pass")}>
        <p>
          {t(
            "Each shift gives ten progress. Every fifty progress unlocks 100 shells. All ten tiers are free. Unclaimed rewards expire at the monthly reset.",
          )}
        </p>
        <p>{t("Season ends: {time}", { time: dateTime(d.season_ends) })}</p>
        <progress value={xp} max={500} aria-label={t("Season progress")} />
        <ol className="economy-track">
          {Array.from({ length: 10 }, (_, i) => i + 1).map((tier) => (
            <li key={tier}>
              <strong>{t("Tier {tier}", { tier })}</strong>
              <span>{t("100 shells")}</span>
              <PixelButton
                disabled={busy || tier !== d.season_claimed + 1 || xp < tier * 50}
                onClick={() => run({ action: "season", tier })}
              >
                {tier <= d.season_claimed ? t("Claimed") : t("Claim reward")}
              </PixelButton>
            </li>
          ))}
        </ol>
      </Panel>
      <Panel title={t("Prestige")}>
        <p>
          {t(
            "After forty shifts, spend 10,000 shells to return to apprentice and gain one permanent prestige level. Inventory, season and daily limits stay. Prestige does not multiply income.",
          )}
        </p>
        <p>{t("Prestige level: {level}", { level: p?.prestige ?? 0 })}</p>
        <label className="economy-check">
          <input type="checkbox" checked={prestige} onChange={(e) => setPrestige(e.target.checked)} />
          {t("I understand: spend 10,000 shells and reset my job rank.")}
        </label>
        <PixelButton
          disabled={busy || !prestige || (p?.shifts ?? 0) < 40}
          onClick={() => {
            run({ action: "prestige", expected: p?.prestige ?? 0 });
            setPrestige(false);
          }}
        >
          {t("Prestige for 10,000 shells")}
        </PixelButton>
      </Panel>
    </>
  );
}
