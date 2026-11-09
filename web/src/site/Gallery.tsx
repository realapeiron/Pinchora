import { translate as spaTranslate } from "../i18n";
import { useRef, useState, type KeyboardEvent } from "react";

import { Crab } from "../components/Crab";
import type { ShotKey } from "./features";
import { SHOTS, altOf } from "./shots";

interface Tab {
  id: string;
  label: string;
  /** What the pretend message says was used. */
  user: string;
  command: string;
  shot: ShotKey;
  title: string;
  text: string;
}

const TABS: Tab[] = [
  {
    id: "profile",
    label: "Profile card",
    user: "captain.pinch",
    command: "/profile",
    shot: "profile",
    title: "A card worth showing off",
    text: "Level, shells, ranks, badges on rarity plates, a character you designed and a showcase of your favourite fish, pet, furniture and house.",
  },
  {
    id: "house",
    label: "House room",
    user: "captain.pinch",
    command: "/house view",
    shot: "room",
    title: "A room you decorated",
    text: "Furniture placed tile by tile in isometric pixel art, with your pet wandering and your character at home. Friends can visit and sign the guestbook.",
  },
  {
    id: "game",
    label: "Game board",
    user: "marlowe",
    command: "/duel chess",
    shot: "chess",
    title: "Games on a real board",
    text: "Reef Chess with sea-creature pieces, plus checkers, connect four, sea fleet, wordle, picross and party games. Moves are menus and buttons under the board.",
  },
  {
    id: "welcome",
    label: "Welcome card",
    user: "Pinchora",
    command: "New member joined",
    shot: "welcome",
    title: "A greeting that feels like home",
    text: "A welcome card with the new member's avatar and number, in your own colours and words. Goodbyes and level-up announcements match.",
  },
];

/** The cards the bot draws, shown the way they arrive in Discord. */
export function Gallery() {
  const [active, setActive] = useState(0);
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);

  const move = (to: number) => {
    const next = (to + TABS.length) % TABS.length;
    setActive(next);
    tabs.current[next]?.focus();
  };
  const onKey = (event: KeyboardEvent) => {
    if (event.key === "ArrowRight") move(active + 1);
    else if (event.key === "ArrowLeft") move(active - 1);
    else if (event.key === "Home") move(0);
    else if (event.key === "End") move(TABS.length - 1);
    else return;
    event.preventDefault();
  };

  return (
    <div className="gallery">
      <div
        className="gallery-tabs"
        role="tablist"
        tabIndex={0}
        aria-label={spaTranslate("What Pinchora draws")}
        onKeyDown={onKey}
      >
        {TABS.map((tab, i) => (
          <button
            key={tab.id}
            ref={(el) => {
              tabs.current[i] = el;
            }}
            type="button"
            role="tab"
            id={`tab-${tab.id}`}
            aria-selected={i === active}
            aria-controls={`panel-${tab.id}`}
            tabIndex={i === active ? 0 : -1}
            className="gallery-tab"
            onClick={() => setActive(i)}
          >
            {spaTranslate(tab.label)}
          </button>
        ))}
      </div>
      {TABS.map((tab, i) => {
        const shot = SHOTS[tab.shot];
        return (
          <div
            key={tab.id}
            role="tabpanel"
            id={`panel-${tab.id}`}
            aria-labelledby={`tab-${tab.id}`}
            hidden={i !== active}
            className="gallery-panel"
          >
            <div className="chat">
              <div className="chat-used">
                <span className="chat-used-arm" aria-hidden="true" />
                <b>{tab.user}</b>
                {spaTranslate("used")} <span className="chat-command">{tab.command}</span>
              </div>
              <div className="chat-message">
                <span className="chat-avatar" aria-hidden="true">
                  <Crab scale={3} />
                </span>
                <div className="chat-body">
                  <div className="chat-head">
                    <span className="chat-name">Pinchora</span>
                    <span className="chat-app">{spaTranslate("APP")}</span>
                    <span className="chat-time">{spaTranslate("Today at 4:20 PM")}</span>
                  </div>
                  <img
                    className="chat-image"
                    src={shot.src}
                    width={shot.width}
                    height={shot.height}
                    alt={spaTranslate(altOf(tab.shot))}
                    loading={i === 0 ? "eager" : "lazy"}
                    decoding="async"
                  />
                </div>
              </div>
            </div>
            <div className="gallery-text">
              <h3 className="pixel-title">{spaTranslate(tab.title)}</h3>
              <p>{spaTranslate(tab.text)}</p>
              <p className="gallery-try muted">
                {" "}
                {spaTranslate("Try")}{" "}
                <code>{tab.command.startsWith("/") ? tab.command : "/setup welcome message"}</code>
              </p>
            </div>
          </div>
        );
      })}
    </div>
  );
}
