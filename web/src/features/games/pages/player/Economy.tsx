import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../../../lib/api";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { Loading, Problem } from "../../../shell/pages/States";
import { t, translate, number, dateTime } from "../../../../i18n";
import {
  useEconomy,
  useEconomyAction,
  provisionReady,
  seasonProgress,
  type Economy,
  type Offer,
  type Trade,
  type Auction,
} from "../../../economy/api/playerEconomy";
import "../../../../styles/player-economy.css";

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
      )}
    </section>
  );
}
type Run = (a: Record<string, unknown>) => void;
function AuctionForm({ d, busy, run }: { d: Economy; busy: boolean; run: Run }) {
  const [item, setItem] = useState("");
  const [price, setPrice] = useState(100);
  const [hours, setHours] = useState(24);
  return (
    <form
      className="economy-form"
      onSubmit={(e) => {
        e.preventDefault();
        run({ action: "auction", item, price, hours });
      }}
    >
      <label>
        {t("Inventory item")}
        <select value={item} onChange={(e) => setItem(e.target.value)} required>
          <option value="">{t("Choose an item")}</option>
          {d.inventory.map((i) => (
            <option key={i.id} value={i.id}>
              {i.name} ({i.quantity})
            </option>
          ))}
        </select>
      </label>
      <label>
        {t("Starting price")}
        <input
          type="number"
          min={1}
          max={10000000}
          value={price}
          onChange={(e) => setPrice(e.target.valueAsNumber)}
          required
        />
      </label>
      <label>
        {t("Hours")}
        <input
          type="number"
          min={1}
          max={72}
          value={hours}
          onChange={(e) => setHours(e.target.valueAsNumber)}
          required
        />
      </label>
      <PixelButton type="submit" disabled={busy || !item}>
        {t("List auction")}
      </PixelButton>
    </form>
  );
}
function AuctionRow({ a, user, busy, run }: { a: Auction; user: string; busy: boolean; run: Run }) {
  const [amount, setAmount] = useState(a.minimum_bid);
  const ended = Date.now() >= Date.parse(a.ends_at);
  return (
    <article>
      <h3>
        {t("Auction #{id}", { id: a.id })} · {a.item_id}
      </h3>
      <p>
        {t("Seller: {id}", { id: a.seller_id })} · {translate(a.status)}
      </p>
      <p>
        {t("Current bid: {amount} shells", { amount: a.bid })} · {dateTime(a.ends_at)}
      </p>
      {a.status === "open" && (
        <div className="economy-form">
          {!ended && a.seller_id !== user && (
            <>
              <label>
                {t("Your bid")}
                <input
                  type="number"
                  min={a.minimum_bid}
                  max={10000000}
                  value={amount}
                  onChange={(e) => setAmount(e.target.valueAsNumber)}
                />
              </label>
              <PixelButton
                disabled={busy || !Number.isInteger(amount) || amount < a.minimum_bid || amount > 10000000}
                onClick={() => run({ action: "bid", id: a.id, amount })}
              >
                {t("Reserve bid")}
              </PixelButton>
            </>
          )}
          {ended ? (
            <PixelButton disabled={busy} onClick={() => run({ action: "settle_auction", id: a.id })}>
              {t("Settle auction")}
            </PixelButton>
          ) : (
            a.seller_id === user &&
            !a.bidder_id && (
              <PixelButton
                variant="secondary"
                disabled={busy}
                onClick={() => run({ action: "cancel_auction", id: a.id })}
              >
                {t("Cancel auction")}
              </PixelButton>
            )
          )}
        </div>
      )}
    </article>
  );
}
function TradeForm({ d, busy, run }: { d: Economy; busy: boolean; run: Run }) {
  const [to, setTo] = useState("");
  const [give, setGive] = useState<Offer>({ shells: 0, items: [] });
  const [receive, setReceive] = useState<Offer>({ shells: 0, items: [] });
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        run({ action: "trade", to, give, receive });
      }}
    >
      <label>
        {t("Partner player ID")}
        <input
          value={to}
          inputMode="numeric"
          pattern="[1-9][0-9]{0,18}"
          required
          onChange={(e) => setTo(e.target.value)}
        />
      </label>
      <div className="economy-columns">
        <OfferEditor label={t("You give")} value={give} onChange={setGive} inventory={d.inventory} />
        <OfferEditor label={t("You request")} value={receive} onChange={setReceive} />
      </div>
      <PixelButton
        type="submit"
        disabled={
          busy ||
          !to ||
          to === d.user_id ||
          (give.shells === 0 && receive.shells === 0 && !give.items.length && !receive.items.length)
        }
      >
        {t("Reserve my offer and create trade")}
      </PixelButton>
    </form>
  );
}
function OfferEditor({
  label,
  value,
  onChange,
  inventory,
}: {
  label: string;
  value: Offer;
  onChange: (v: Offer) => void;
  inventory?: Economy["inventory"];
}) {
  const [item, setItem] = useState("");
  const [qty, setQty] = useState(1);
  return (
    <fieldset>
      <legend>{label}</legend>
      <label>
        {t("Shells")}
        <input
          type="number"
          min={0}
          max={10000000}
          value={value.shells}
          onChange={(e) => onChange({ ...value, shells: e.target.valueAsNumber })}
          required
        />
      </label>
      <ul>
        {value.items.map((i) => (
          <li key={i.item_id}>
            {number(i.quantity)} × {i.item_id}{" "}
            <PixelButton
              variant="ghost"
              onClick={() =>
                onChange({ ...value, items: value.items.filter((x) => x.item_id !== i.item_id) })
              }
            >
              {t("Remove")}
            </PixelButton>
          </li>
        ))}
      </ul>
      <label>
        {t("Item identifier")}
        {inventory ? (
          <select value={item} onChange={(e) => setItem(e.target.value)}>
            <option value="">{t("Choose an item")}</option>
            {inventory.map((i) => (
              <option key={i.id} value={i.id}>
                {i.name}
              </option>
            ))}
          </select>
        ) : (
          <input value={item} maxLength={100} onChange={(e) => setItem(e.target.value)} />
        )}
      </label>
      <label>
        {t("Quantity")}
        <input type="number" min={1} max={10} value={qty} onChange={(e) => setQty(e.target.valueAsNumber)} />
      </label>
      <PixelButton
        variant="secondary"
        disabled={
          !item ||
          !Number.isInteger(qty) ||
          qty < 1 ||
          qty > 10 ||
          value.items.length >= 8 ||
          value.items.some((i) => i.item_id === item)
        }
        onClick={() => {
          onChange({ ...value, items: [...value.items, { item_id: item, quantity: qty }] });
          setItem("");
        }}
      >
        {t("Add item to offer")}
      </PixelButton>
    </fieldset>
  );
}
export function TradeReview({ trade: tr, busy, run }: { trade: Trade; busy: boolean; run: Run }) {
  const [checked, setChecked] = useState(false);
  return (
    <section className="economy-review" aria-label={t("Exact trade review")}>
      <h3>{t("Check both sides before confirming")}</h3>
      <div className="economy-columns">
        {[
          [tr.user_a, tr.offer_a, tr.confirmed_a],
          [tr.user_b, tr.offer_b, tr.confirmed_b],
        ].map(([id, offer, confirmed]) => (
          <div key={String(id)}>
            <h4>{t("Player {id}", { id: String(id) })}</h4>
            <p>{t("{amount} shells", { amount: (offer as Offer).shells })}</p>
            <ul>
              {(offer as Offer).items.map((i) => (
                <li key={i.item_id}>
                  {number(i.quantity)} × {i.item_id}
                </li>
              ))}
            </ul>
            <p>{confirmed ? t("Confirmed") : t("Awaiting confirmation")}</p>
          </div>
        ))}
      </div>
      <p>{t("Expires: {time}", { time: dateTime(tr.expires_at) })}</p>
      {tr.status === "open" && (
        <>
          <label className="economy-check">
            <input type="checkbox" checked={checked} onChange={(e) => setChecked(e.target.checked)} />
            {t("I checked both player IDs, item quantities and shell amounts.")}
          </label>
          <div className="player-actions">
            <PixelButton
              disabled={busy || !checked}
              onClick={() => {
                run({ action: "confirm_trade", id: tr.id });
                setChecked(false);
              }}
            >
              {t("Confirm this exact trade")}
            </PixelButton>
            <PixelButton
              variant="secondary"
              disabled={busy}
              onClick={() => run({ action: "cancel_trade", id: tr.id })}
            >
              {t("Cancel and refund")}
            </PixelButton>
          </div>
        </>
      )}
    </section>
  );
}

function DonationPool({ busy, run }: { busy: boolean; run: Run }) {
  const [guild, setGuild] = useState("");
  const [selected, setSelected] = useState("");
  const [amount, setAmount] = useState(100);
  const [confirm, setConfirm] = useState(false);
  const pool = useQuery({
    queryKey: ["economy", "pool", selected],
    queryFn: () => api<{ pool_total: number; pool_goal: number }>(`/api/me/economy/pool?guild=${selected}`),
    enabled: !!selected,
  });
  return (
    <Panel title={t("Community pool")}>
      <p>
        {t(
          "Donate to a server you belong to. Contributions are permanent, with no withdrawals or personal payouts.",
        )}
      </p>
      <form
        className="economy-form"
        onSubmit={(e) => {
          e.preventDefault();
          setSelected(guild);
          setConfirm(false);
        }}
      >
        <label>
          {t("Server ID")}
          <input
            value={guild}
            onChange={(e) => setGuild(e.target.value)}
            pattern="[1-9][0-9]{0,18}"
            required
          />
        </label>
        <PixelButton type="submit" variant="secondary">
          {t("View goal")}
        </PixelButton>
      </form>
      {pool.isError && <Problem error={pool.error} />}{" "}
      {pool.data && (
        <>
          <p>
            {number(pool.data.pool_total)} / {number(pool.data.pool_goal)} 🐚
          </p>
          <progress
            value={Math.min(pool.data.pool_total, pool.data.pool_goal)}
            max={pool.data.pool_goal}
            aria-label={t("Donation goal progress")}
          />
          {pool.data.pool_total >= pool.data.pool_goal && <p>{t("Community goal reached!")}</p>}
          <label>
            {t("Shells to donate")}
            <input
              type="number"
              min={1}
              max={10000000}
              value={amount}
              onChange={(e) => {
                setAmount(e.target.valueAsNumber);
                setConfirm(false);
              }}
            />
          </label>
          <label className="economy-check">
            <input type="checkbox" checked={confirm} onChange={(e) => setConfirm(e.target.checked)} />
            {t("I understand this donation is permanent.")}
          </label>
          <PixelButton
            disabled={busy || !confirm || !Number.isInteger(amount) || amount < 1 || amount > 10000000}
            onClick={() => {
              run({ action: "donate", guild: selected, amount });
              setConfirm(false);
            }}
          >
            {t("Donate shells")}
          </PixelButton>
        </>
      )}
    </Panel>
  );
}
