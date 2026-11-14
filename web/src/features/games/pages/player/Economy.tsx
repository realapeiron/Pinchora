import { useState } from "react";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { Loading, Problem } from "../../../../components/States";
import { t, translate } from "../../../../i18n";
import { useEconomy, useEconomyAction, seasonProgress } from "../../../economy";
import { AuctionForm } from "./economy/AuctionForm";
import { AuctionRow } from "./economy/AuctionRow";
import { TradeForm } from "./economy/TradeForm";
import { TradeReview } from "./economy/TradeReview";
import { DonationPool } from "./economy/DonationPool";
import { JobsPanel } from "./economy/JobsPanel";
import { SeasonPanel } from "./economy/SeasonPanel";
import "../../../../styles/player-economy.css";

export function EconomyPanels() {
  const q = useEconomy();
  const action = useEconomyAction();
  const [tab, setTab] = useState("jobs");
  const [review, setReview] = useState<number | null>(null);
  const [prestige, setPrestige] = useState(false);
  const d = q.data;
  const p = d?.player;
  const busy = action.isPending;
  const run = (command: Record<string, unknown>) => action.mutate(command);
  if (q.isPending) return <Loading />;
  if (q.isError) return <Problem error={q.error} />;
  if (!d) return null;
  const now = Date.now();
  const shiftActive = !!p?.shift_started && p.shift_step < 3 && now < Date.parse(p.shift_started) + 300000;
  const ready = d.shifts_today < 3 && (!d.next_shift || now >= Date.parse(d.next_shift));
  const xp = seasonProgress(d);
  return (
    <section className="economy-v2" aria-label={t("Reef economy")}>
      <nav className="economy-tabs" aria-label={t("Wallet activities")}>
        {[
          ["jobs", "Jobs & provisions"],
          ["auctions", "Auctions"],
          ["trades", "Trades"],
          ["season", "Season & prestige"],
        ].map(([key, label]) => (
          <PixelButton
            key={key}
            variant={tab === key ? "primary" : "secondary"}
            aria-pressed={tab === key}
            onClick={() => setTab(key ?? "jobs")}
          >
            {translate(label ?? "")}
          </PixelButton>
        ))}
      </nav>
      {action.isError && <Problem error={action.error} />}
      {action.isSuccess && <p role="status">{t("Saved. Your wallet and activity are up to date.")}</p>}
      {tab === "jobs" && (
        <JobsPanel d={d} busy={busy} run={run} ready={ready} shiftActive={shiftActive} now={now} />
      )}
      {tab === "auctions" && (
        <Panel title={t("Dock auctions")}>
          <p>
            {t(
              "Bids reserve shells immediately. Outbid offers are refunded. Minimum increase: 5%, at least one shell. Late bids extend by up to two minutes, at most thirty extra minutes. Sale fee: 5%, rounded up.",
            )}
          </p>
          <AuctionForm d={d} busy={busy} run={run} />
          <div className="economy-list">
            {d.auctions.length ? (
              d.auctions.map((a) => <AuctionRow key={a.id} a={a} user={d.user_id} busy={busy} run={run} />)
            ) : (
              <p>{t("No auctions yet. List an unplaced inventory item to begin.")}</p>
            )}
          </div>
        </Panel>
      )}
      {tab === "trades" && (
        <Panel title={t("Swap table")}>
          <p>
            {t(
              "Offers never change after creation. Your offer is reserved now; your partner reserves theirs when confirming. Both must confirm. Cancel or wait 24 hours for a refund.",
            )}
          </p>
          <TradeForm d={d} busy={busy} run={run} />
          <div className="economy-list">
            {d.trades.length ? (
              d.trades.map((tr) => (
                <article key={tr.id}>
                  <p>
                    {t("Trade #{id}", { id: tr.id })} · {translate(tr.status)} ·{" "}
                    {tr.user_a === d.user_id ? tr.user_b : tr.user_a}
                  </p>
                  <PixelButton variant="secondary" onClick={() => setReview(tr.id)}>
                    {t("Review trade")}
                  </PixelButton>
                  {review === tr.id && <TradeReview trade={tr} busy={busy} run={run} />}
                </article>
              ))
            ) : (
              <p>{t("No trades yet. Create an offer or ask your partner to invite your player ID.")}</p>
            )}
          </div>
          <p>{t("Your player ID: {id}", { id: d.user_id })}</p>
        </Panel>
      )}
      {tab === "jobs" && <DonationPool busy={busy} run={run} />}
      {tab === "season" && (
        <SeasonPanel d={d} busy={busy} run={run} xp={xp} prestige={prestige} setPrestige={setPrestige} />
      )}
    </section>
  );
}

export { TradeReview } from "./economy/TradeReview";
