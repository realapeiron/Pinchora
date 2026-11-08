import { useState } from "react";
import { useParams } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { settingsDraft, type Settings, type PointsPreview } from "../lib/economyV2";
import { t, number } from "../i18n";
import { Panel } from "../components/Panel";
import { PixelButton } from "../components/PixelButton";
import { SettingsPage } from "./PageHead";
import { Loading, Problem } from "./States";
import "../styles/economy-v2.css";
export function EconomySettings() {
  const { guildId } = useParams();
  const path = `/api/guilds/${guildId}/economy`;
  const q = useQuery({ queryKey: ["economy-settings", guildId], queryFn: () => api<Settings>(path) });
  return (
    <SettingsPage
      title={t("Economy settings")}
      lead={t(
        "Set store conditions and a community goal. Reset and inflation tools affect server points only; global shells stay with players.",
      )}
    >
      {q.isPending ? (
        <Loading />
      ) : q.isError ? (
        <Problem error={q.error} />
      ) : (
        <SettingsEditor key={`${guildId}:${q.data.revision}`} initial={q.data} path={path} />
      )}
    </SettingsPage>
  );
}
function SettingsEditor({ initial, path }: { initial: Settings; path: string }) {
  const client = useQueryClient();
  const [draft, setDraft] = useState(settingsDraft(initial));
  const [percent, setPercent] = useState(100);
  const [review, setReview] = useState<PointsPreview | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const dirty = JSON.stringify(draft) !== JSON.stringify(settingsDraft(initial));
  const save = useMutation({
    mutationFn: () => api<Settings>(path, "PUT", { settings: draft, revision: initial.revision }),
    onSuccess: () => client.invalidateQueries({ queryKey: ["economy-settings"] }),
  });
  const preview = useMutation({
    mutationFn: () => api<PointsPreview>(`${path}/points`),
    onSuccess: (v) => {
      setReview(v);
      setConfirmed(false);
    },
  });
  const scale = useMutation({
    mutationFn: () => api<PointsPreview>(`${path}/points`, "POST", { percent, version: review?.version }),
    onSuccess: () => {
      setReview(null);
      setConfirmed(false);
    },
  });
  return (
    <div className="economy-v2">
      <Panel title={t("Community pool")}>
        <p>
          {t(
            "Donated shells are permanent community contributions. Reaching the goal marks a milestone; the pool never pays shells back out.",
          )}
        </p>
        <p>
          {number(initial.pool_total)} / {number(initial.pool_goal)} 🐚
        </p>
        <progress
          value={Math.min(initial.pool_total, initial.pool_goal)}
          max={initial.pool_goal}
          aria-label={t("Donation goal progress")}
        />
        {initial.pool_total >= initial.pool_goal && <p role="status">{t("Community goal reached!")}</p>}
      </Panel>
      <Panel title={t("Store conditions")}>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (dirty && !save.isPending) save.mutate();
          }}
        >
          <div className="economy-form">
            <label>
              {t("Donation goal")}
              <input
                type="number"
                min={100}
                max={1000000000}
                value={draft.pool_goal}
                required
                onChange={(e) => setDraft({ ...draft, pool_goal: e.target.valueAsNumber })}
              />
            </label>
            <label>
              {t("Minimum server level")}
              <input
                type="number"
                min={0}
                max={100}
                value={draft.shop_min_level}
                required
                onChange={(e) => setDraft({ ...draft, shop_min_level: e.target.valueAsNumber })}
              />
            </label>
            <label>
              {t("Minimum membership days")}
              <input
                type="number"
                min={0}
                max={365}
                value={draft.shop_min_days}
                required
                onChange={(e) => setDraft({ ...draft, shop_min_days: e.target.valueAsNumber })}
              />
            </label>
            <label className="economy-check">
              <input
                type="checkbox"
                checked={draft.shop_enabled}
                onChange={(e) => setDraft({ ...draft, shop_enabled: e.target.checked })}
              />
              {t("Store enabled")}
            </label>
          </div>
          <p>
            {t(
              "Membership days count from the first stored membership record after this feature was enabled.",
            )}
          </p>
          <PixelButton type="submit" disabled={!dirty || save.isPending}>
            {t("Save changes")}
          </PixelButton>
          {save.isError && <Problem error={save.error} />}
        </form>
      </Panel>
      <Panel title={t("Server-point reset & inflation")}>
        <p>
          {t(
            "Preview all affected members. Choose the percentage of server points to keep: zero resets points, fifty keeps half. Concurrent balance changes require a new preview.",
          )}
        </p>
        <label>
          {t("Percentage to keep")}
          <input
            type="number"
            min={0}
            max={100}
            value={percent}
            onChange={(e) => {
              setPercent(e.target.valueAsNumber);
              setConfirmed(false);
            }}
          />
        </label>
        <PixelButton variant="secondary" disabled={preview.isPending} onClick={() => preview.mutate()}>
          {t("Preview affected balances")}
        </PixelButton>
        {preview.isError && <Problem error={preview.error} />}
        {review && (
          <div className="economy-review">
            <p>
              {t("{count} members · {total} server points · keep {percent}%", {
                count: review.count,
                total: review.total,
                percent,
              })}
            </p>
            <label className="economy-check">
              <input type="checkbox" checked={confirmed} onChange={(e) => setConfirmed(e.target.checked)} />
              {t("I confirm this server-point change for the displayed members.")}
            </label>
            <PixelButton
              disabled={
                !confirmed ||
                scale.isPending ||
                !Number.isInteger(percent) ||
                percent < 0 ||
                percent >= 100 ||
                review.count === 0 ||
                review.total === "0"
              }
              onClick={() => scale.mutate()}
            >
              {percent === 0 ? t("Reset server points") : t("Apply point reduction")}
            </PixelButton>
          </div>
        )}
        {scale.isError && <Problem error={scale.error} />}{" "}
        {scale.isSuccess && <p role="status">{t("Server points updated.")}</p>}
      </Panel>
    </div>
  );
}
