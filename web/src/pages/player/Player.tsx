import { NavLink, Route, Routes } from "react-router";
import { type ReactNode } from "react";
import { TopBar } from "../../components/TopBar";
import { Panel } from "../../components/Panel";
import { PixelLink } from "../../components/PixelButton";
import { t, number, dateTime, useLocale } from "../../i18n";
import type { Me } from "../../lib/api";
import { usePlayer, type HomeData, type WalletData } from "../../lib/webPlayer";
import { PageHead } from "../PageHead";
import { RequireLogin } from "../RequireLogin";
import { Loading, Problem } from "../States";
import { ProfileStudio } from "./ProfileStudio";
import { CharacterStudio } from "./CharacterStudio";
import { Friends, Guestbook, FishBook, Inventory, Pets, Quests, Stall } from "./Depth";
import "../../styles/web-player.css";

export function PlayerRoutes() {
  useLocale();
  return (
    <RequireLogin>
      {(me) => (
        <PlayerShell me={me}>
          <Routes>
            <Route index element={<PlayerHome />} />
            <Route path="profile" element={<ProfileStudio />} />
            <Route path="character" element={<CharacterStudio />} />
            <Route path="wallet" element={<Wallet />} />
            <Route path="inventory" element={<Inventory />} />
            <Route path="pets" element={<Pets />} />
            <Route path="stall" element={<Stall />} />
            <Route path="fishbook" element={<FishBook />} />
            <Route path="quests" element={<Quests />} />
            <Route path="guestbook" element={<Guestbook />} />
            <Route path="friends" element={<Friends />} />
          </Routes>
        </PlayerShell>
      )}
    </RequireLogin>
  );
}
function PlayerShell({ me, children }: { me: Me; children: ReactNode }) {
  return (
    <div className="page">
      <TopBar me={me} />
      <main className="content player-content">
        <nav className="player-nav" aria-label={t("Player pages")}>
          <NavLink to="/me" end>
            {t("My world")}
          </NavLink>
          <NavLink to="/me/profile">{t("Profile studio")}</NavLink>
          <NavLink to="/me/character">{t("Character")}</NavLink>
          <NavLink to="/me/wallet">{t("Wallet")}</NavLink>
          <NavLink to="/me/house">{t("My house")}</NavLink>
          <NavLink to="/me/inventory">{t("Inventory")}</NavLink>
          <NavLink to="/me/pets">{t("Pets")}</NavLink>
          <NavLink to="/me/stall">{t("Stall")}</NavLink>
          <NavLink to="/me/fishbook">{t("Fish book")}</NavLink>
          <NavLink to="/me/quests">{t("Quests")}</NavLink>
          <NavLink to="/me/guestbook">{t("Guestbook")}</NavLink>
          <NavLink to="/me/friends">{t("Friends")}</NavLink>
        </nav>
        {children}
      </main>
    </div>
  );
}
function PlayerHome() {
  const language = useLocale();
  const query = usePlayer<HomeData>("home");
  const quests = usePlayer<{ items: { progress: number; target: number; claimed: boolean }[] }>("quests");
  return (
    <>
      <PageHead title={t("My world")} lead={t("Your corner of Pinchora. Make yourself at home.")} />
      {query.isPending ? (
        <Loading />
      ) : query.isError ? (
        <Problem error={query.error} />
      ) : (
        <>
          <div className="player-home-grid">
            <section className="player-portrait">
              <img
                src={`/api/me/preview/profile-card.png?lang=${language}`}
                alt={t("Your profile card")}
                className="player-card"
              />
              <div className="player-actions">
                <PixelLink href="/me/profile">{t("Edit profile card")}</PixelLink>
                <PixelLink href="/me/character" variant="secondary">
                  {t("Dress your character")}
                </PixelLink>
              </div>
            </section>
            <Panel title={t("At a glance")}>
              <dl className="player-facts">
                <div>
                  <dt>{t("Shells")}</dt>
                  <dd>{number(query.data.shells)}</dd>
                </div>
                <div>
                  <dt>{t("Pearls")}</dt>
                  <dd>{number(query.data.pearls)}</dd>
                </div>
                <div>
                  <dt>{t("Level")}</dt>
                  <dd>{number(query.data.level)}</dd>
                </div>
                <div>
                  <dt>{t("Daily streak")}</dt>
                  <dd>{t("{count} days", { count: query.data.streak })}</dd>
                </div>
              </dl>
              <progress
                aria-label={t("Level progress")}
                value={query.data.xp_current}
                max={query.data.xp_needed}
              />
              <p className="muted">
                {number(query.data.xp_current)} / {number(query.data.xp_needed)} XP
              </p>
              <PixelLink href="/me/wallet" variant="secondary">
                {t("Open wallet")}
              </PixelLink>
              {quests.data && (
                <p className="muted">
                  {t("{count} quests ready", {
                    count: quests.data.items.filter((q) => q.progress >= q.target && !q.claimed).length,
                  })}
                </p>
              )}
              <PixelLink href="/me/quests" variant="secondary">
                {t("Open quests")}
              </PixelLink>
            </Panel>
          </div>
          {(query.data.birthday || query.data.roblox) && (
            <div className="player-home-grid">
              {query.data.birthday && (
                <Panel title={t("Birthday")}>
                  <p>
                    {query.data.birthday.day}/{query.data.birthday.month}
                    {query.data.birthday.year ? `/${query.data.birthday.year}` : ""} ·{" "}
                    {query.data.birthday.tz}
                  </p>
                  <p className="muted">
                    {query.data.birthday.public ? t("Visible in servers") : t("Private")}
                  </p>
                </Panel>
              )}
              {query.data.roblox && (
                <Panel title={t("Roblox link")}>
                  <p>{query.data.roblox.name}</p>
                  <p className="muted">
                    {query.data.roblox.method === "oauth"
                      ? t("Connected with Roblox")
                      : t("Verified with profile code")}
                  </p>
                </Panel>
              )}
            </div>
          )}
          <div className="player-home-grid">
            <Panel title={t("My house")}>
              {query.data.house ? (
                <>
                  <img
                    className="player-house"
                    src={`/api/house/rooms/${query.data.house.room}/render.png`}
                    alt={query.data.house.name}
                  />
                  <PixelLink href="/me/house">{t("Visit your house")}</PixelLink>
                </>
              ) : (
                <p>{t("Open /house in Discord to start your home.")}</p>
              )}
            </Panel>
            <Panel title={t("Your pets")}>
              {query.data.pets.length ? (
                <ul className="player-pets">
                  {query.data.pets.map((p) => (
                    <li key={p.id}>{p.name}</li>
                  ))}
                </ul>
              ) : (
                <p className="muted">{t("No pets yet. Meet your first companion with /pet in Discord.")}</p>
              )}
              <p>{t("Collect daily rewards and check your quests in Discord.")}</p>
              <div className="player-actions">
                <PixelLink href="/me/pets" variant="secondary">
                  {t("Open pets")}
                </PixelLink>
                <PixelLink href={query.data.discord_url} variant="secondary">
                  {t("Open in Discord")}
                </PixelLink>
              </div>
            </Panel>
          </div>
        </>
      )}
    </>
  );
}
function Wallet() {
  const language = useLocale();
  const query = usePlayer<WalletData>("wallet");
  return (
    <>
      <PageHead title={t("Wallet")} lead={t("Your shells, pearls, and latest 50 shell transactions.")} />
      {query.isPending ? (
        <Loading />
      ) : query.isError ? (
        <Problem error={query.error} />
      ) : (
        <>
          <img
            src={`/api/me/preview/wallet-card.png?lang=${language}`}
            className="player-wallet"
            alt={t("Your wallet card")}
          />
          <Panel title={t("Recent transactions")}>
            <p>
              {t("{shells} shells · {pearls} pearls", {
                shells: query.data.shells,
                pearls: query.data.pearls,
              })}
            </p>
            {!query.data.ledger.length ? (
              <p className="muted">{t("Your first shell transaction will appear here.")}</p>
            ) : (
              <ol className="player-ledger">
                {query.data.ledger.map((entry, i) => (
                  <li key={i}>
                    <div>
                      <strong>{entry.delta > 0 ? t("Shells received") : t("Shells spent")}</strong>
                      <time dateTime={entry.created_at}>{dateTime(entry.created_at)}</time>
                    </div>
                    <b className={entry.delta > 0 ? "player-income" : ""}>
                      {entry.delta > 0 ? "+" : ""}
                      {number(entry.delta)}
                    </b>
                  </li>
                ))}
              </ol>
            )}
          </Panel>
        </>
      )}
    </>
  );
}
