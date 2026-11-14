import { useState } from "react";
import { Panel } from "../../../../../components/Panel";
import { PixelButton } from "../../../../../components/PixelButton";
import { PageHead } from "../../../../../components/PageHead";
import { Loading, Problem } from "../../../../../components/States";
import { t, number } from "../../../../../i18n";
import { usePlayer, petRename } from "../../../api/webPlayer";
import { useAction } from "./useAction";
import { Feedback } from "./Feedback";
import { Empty } from "./Empty";
import { petStatus, needName } from "./petLabels";

type Pet = {
  id: string;
  name: string;
  species_name: string;
  level: number;
  xp: number;
  active: boolean;
  status: string;
  needs: Record<string, number>;
  trip_dest: string | null;
};

export function Pets() {
  const q = usePlayer<{ items: Pet[]; trips: { id: string; name: string; hours: number; level: number }[] }>(
    "pets",
  );
  const action = useAction("/api/me/pets");
  const [names, setNames] = useState<Record<string, string>>({});
  const [destinations, setDestinations] = useState<Record<string, string>>({});
  return (
    <>
      <PageHead title={t("Pets")} lead={t("Look after your companions and watch their needs.")} />
      {q.isPending ? (
        <Loading />
      ) : q.isError ? (
        <Problem error={q.error} />
      ) : q.data.items.length ? (
        <div className="player-depth-grid">
          {q.data.items.map((p) => (
            <Panel key={p.id} title={p.name}>
              <img
                className="player-pet-card"
                src={`/api/me/pets/${p.id}/card.png?v=${p.xp}-${Object.values(p.needs).join("-")}`}
                alt={`${p.name} ${p.species_name}`}
              />
              <p>
                {p.species_name} · {t("Level")} {number(p.level)} {p.active ? `· ${t("Active")}` : ""}
              </p>
              <p className="muted">{petStatus(p.status)}</p>
              <div className="player-needs">
                {Object.entries(p.needs).map(([key, value]) => (
                  <label key={key}>
                    {needName(key)}
                    <progress value={value} max={100} />
                    {number(value)}
                  </label>
                ))}
              </div>
              <div className="player-depth-actions">
                {(["feed", "play", "bathe", "walk"] as const).map((v) => (
                  <PixelButton
                    key={v}
                    disabled={action.isPending || p.status === "away" || p.status === "asleep"}
                    onClick={() => action.mutate({ suffix: `/${p.id}/care`, body: { action: v } })}
                  >
                    {t(v)}
                  </PixelButton>
                ))}
                {p.status === "asleep" && (
                  <PixelButton onClick={() => action.mutate({ suffix: `/${p.id}/wake` })}>
                    {t("Wake")}
                  </PixelButton>
                )}
                {!p.active && (
                  <PixelButton onClick={() => action.mutate({ suffix: `/${p.id}/active` })}>
                    {t("Make active")}
                  </PixelButton>
                )}
              </div>
              <div className="player-depth-filters">
                <label>
                  {t("Pet name")}
                  <input
                    value={names[p.id] ?? p.name}
                    maxLength={20}
                    onChange={(e) => setNames({ ...names, [p.id]: e.target.value })}
                  />
                </label>
                <PixelButton
                  disabled={action.isPending || !petRename(p.name, names[p.id])}
                  onClick={() =>
                    action.mutate({
                      suffix: `/${p.id}`,
                      method: "PATCH",
                      body: petRename(p.name, names[p.id]),
                    })
                  }
                >
                  {t("Rename")}
                </PixelButton>
              </div>
              {p.status === "awake" && (
                <div className="player-depth-filters">
                  <label>
                    {t("Trip destination")}
                    <select
                      value={destinations[p.id] ?? ""}
                      onChange={(e) => setDestinations({ ...destinations, [p.id]: e.target.value })}
                    >
                      <option value="">{t("Trip destination")}</option>
                      {q.data.trips
                        .filter((v) => p.level >= v.level)
                        .map((v) => (
                          <option key={v.id} value={v.id}>
                            {v.name} · {v.hours} h
                          </option>
                        ))}
                    </select>
                  </label>
                  <PixelButton
                    disabled={!destinations[p.id] || action.isPending}
                    onClick={() => {
                      const trip = q.data.trips.find((v) => v.id === destinations[p.id]);
                      if (trip)
                        action.mutate({
                          suffix: `/${p.id}/trip`,
                          body: { dest: trip.id, hours: trip.hours },
                        });
                    }}
                  >
                    {t("Send on trip")}
                  </PixelButton>
                </div>
              )}
            </Panel>
          ))}
        </div>
      ) : (
        <Empty />
      )}
      {q.data?.items.some((p) => p.status === "back" || (p.status === "asleep" && p.trip_dest)) && (
        <PixelButton onClick={() => action.mutate({ suffix: "/claim" })}>{t("Claim souvenirs")}</PixelButton>
      )}
      <Feedback action={action} />
    </>
  );
}
