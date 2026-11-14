import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../../../../lib/api";
import { Panel } from "../../../../../components/Panel";
import { PixelButton } from "../../../../../components/PixelButton";
import { Problem } from "../../../../../components/States";
import { t, number } from "../../../../../i18n";
import type { Run } from "../../../../economy";

export function DonationPool({ busy, run }: { busy: boolean; run: Run }) {
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
