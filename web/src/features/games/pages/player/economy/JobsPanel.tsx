import { Panel } from "../../../../../components/Panel";
import { PixelButton } from "../../../../../components/PixelButton";
import { t, translate, dateTime } from "../../../../../i18n";
import { provisionReady, type Economy } from "../../../../economy";
import type { Run } from "../../../../economy";

const JOBS = ["kelp_keeper", "coral_courier", "beacon_tender"] as const;
const JOB_NAMES = {
  kelp_keeper: "Kelp keeper",
  coral_courier: "Coral courier",
  beacon_tender: "Beacon tender",
};
const TASKS: Record<string, string[]> = {
  kelp_keeper: ["Sort the fronds", "Tie the bundles", "Stock the reef pantry"],
  coral_courier: ["Read the tide map", "Pack the coral parcel", "Deliver to the dock"],
  beacon_tender: ["Clean the lantern glass", "Align the reflector", "Light the safe passage"],
};

export function JobsPanel({
  d,
  busy,
  run,
  ready,
  shiftActive,
  now,
}: {
  d: Economy;
  busy: boolean;
  run: Run;
  ready: boolean;
  shiftActive: boolean;
  now: number;
}) {
  const p = d.player;
  return (
    <>
      <Panel title={t("Reef jobs")}>
        <p>
          {t(
            "Three shifts daily. One hour between shifts. Promotions every ten shifts; 100–200 shells per shift.",
          )}
        </p>
        <div className="economy-job-board">
          {JOBS.map((job) => (
            <article key={job}>
              <h3>{translate(JOB_NAMES[job])}</h3>
              <p>{translate(TASKS[job]?.[0] ?? "")}</p>
              <PixelButton
                disabled={busy || !ready || shiftActive}
                onClick={() => run({ action: "start_shift", job })}
              >
                {t("Start shift")}
              </PixelButton>
            </article>
          ))}
        </div>
        <p>
          {t("{done}/3 shifts today · rank {rank}/5", {
            done: d.shifts_today,
            rank: Math.min(5, Math.floor((p?.shifts ?? 0) / 10) + 1),
          })}
        </p>
        {d.next_shift && !ready && <p>{t("Next shift: {time}", { time: dateTime(d.next_shift) })}</p>}
        {shiftActive && p && (
          <div className="economy-task">
            <progress value={p.shift_step} max={3} aria-label={t("Shift progress")} />
            <p>{t("Complete three tasks within five minutes.")}</p>
            <PixelButton
              disabled={busy}
              onClick={() => run({ action: "step", token: p.shift_token, step: p.shift_step })}
            >
              {translate(TASKS[p.job]?.[p.shift_step] ?? "")}
            </PixelButton>
          </div>
        )}
      </Panel>
      <div className="economy-columns">
        <Panel title={t("Provisions")}>
          <p>
            {t(
              "Tend the reef pantry, then collect ten shells per full hour. Growth stops at 120 after twelve hours; collect all before tending again.",
            )}
          </p>
          <p>{t("{amount} shells ready", { amount: provisionReady(p ?? null, now) })}</p>
          <div className="player-actions">
            <PixelButton
              disabled={busy || (!!p?.provision_at && p.provision_claimed < 120)}
              onClick={() => run({ action: "tend" })}
            >
              {t("Tend provisions")}
            </PixelButton>
            <PixelButton
              disabled={busy || provisionReady(p ?? null, now) === 0}
              onClick={() => run({ action: "provisions" })}
            >
              {t("Collect provisions")}
            </PixelButton>
          </div>
        </Panel>
        <Panel title={t("Earned treasure")}>
          <p>{t("One chest per five shifts. Hold up to ten. Chests are never sold or traded.")}</p>
          <ul>
            {d.odds.map(([amount, chance]) => (
              <li key={amount}>{t("{chance}% chance: {amount} shells", { chance, amount })}</li>
            ))}
          </ul>
          <PixelButton disabled={busy || !p?.chests} onClick={() => run({ action: "chest" })}>
            {t("Open chest ({count})", { count: p?.chests ?? 0 })}
          </PixelButton>
        </Panel>
      </div>
    </>
  );
}
