import { date, translate as spaTranslate } from "../i18n";
import type { ReactNode } from "react";
import { Link } from "react-router";

import { SUPPORT_URL } from "./content";

/** When both pages were last written; change it with the words. */
const UPDATED = "2026-11-06T12:00:00Z";

interface Section {
  title: string;
  body: ReactNode;
}

function LegalPage({ title, intro, sections }: { title: string; intro: string; sections: Section[] }) {
  return (
    <div className="wrap wrap-narrow legal">
      <header className="page-intro">
        <p className="pixel-label eyebrow">{spaTranslate("LEGAL")}</p>
        <h1 className="section-title">{title}</h1>
        <p className="section-lead">{intro}</p>
        <p className="muted legal-updated">
          {" "}
          {spaTranslate("Last updated")} {date(UPDATED)}
          {spaTranslate(". This is a plain-language draft that will be reviewed before launch.")}{" "}
        </p>
      </header>
      <div className="legal-body">
        {sections.map((s, i) => (
          <section key={s.title} aria-labelledby={`legal-${i}`}>
            <h2 id={`legal-${i}`} className="pixel-title">
              {s.title}
            </h2>
            {s.body}
          </section>
        ))}
      </div>
    </div>
  );
}

export function Privacy() {
  return (
    <LegalPage
      title={spaTranslate("Privacy policy")}
      intro={spaTranslate(
        "Pinchora is a game and a toolbox inside Discord. This page says what it keeps about you and your server, and how to get rid of it.",
      )}
      sections={[
        {
          title: spaTranslate("What Pinchora stores about players"),
          body: (
            <>
              <p>{spaTranslate("Your Discord user ID, and what you do with the bot:")}</p>
              <ul>
                <li>
                  {spaTranslate(
                    "experience points, levels, shells and other currencies, streaks and reward timers;",
                  )}
                </li>
                <li>{spaTranslate("your bio, card style, character, badges and titles;")}</li>
                <li>{spaTranslate("your house and furniture, pets, fish book, friends and partner;")}</li>
                <li>
                  {" "}
                  {spaTranslate(
                    "game results and statistics, reminders you set, notes you sign in a guestbook, suggestions, polls and giveaway entries you make.",
                  )}{" "}
                </li>
              </ul>
              <p>
                {" "}
                {spaTranslate(
                  "Pinchora counts messages to give chat XP. It does not store the text of what people write in chat. Names and avatars are read from Discord when a card is drawn.",
                )}{" "}
              </p>
            </>
          ),
        },
        {
          title: spaTranslate("What Pinchora stores about servers"),
          body: (
            <p>
              {" "}
              {spaTranslate(
                "The server ID, the settings its managers choose (channels, roles, XP rules, the store, which parts are switched on) and a log of changes made in the dashboard or with commands: who changed what, with the values before and after. Log entries are deleted after 180 days.",
              )}{" "}
            </p>
          ),
        },
        {
          title: spaTranslate("The website and the dashboard"),
          body: (
            <>
              <p>
                {" "}
                {spaTranslate(
                  "The public pages set no cookies and load no third-party scripts, trackers or advertising. Fonts and pictures come from this site (once you log in, your avatar comes from Discord).",
                )}{" "}
              </p>
              <p>
                {" "}
                {spaTranslate(
                  "When you log in to the dashboard with Discord, Pinchora receives your user ID, username, avatar and the list of servers you are in, so it can show the ones you manage. They are kept in a session on Pinchora's servers behind one cookie that the page's scripts cannot read. Discord's access tokens never reach your browser. A session ends after seven days without use, or after thirty days.",
                )}{" "}
              </p>
            </>
          ),
        },
        {
          title: spaTranslate("How your choices work"),
          body: (
            <p>
              {" "}
              {spaTranslate("The")} <code>/privacy</code>{" "}
              {spaTranslate(
                "command makes your profile private, leaves you off the global boards and hides your house.",
              )}{" "}
              <b>{spaTranslate("Delete my data")}</b>{" "}
              {spaTranslate(
                "in the same panel removes every row about you after two confirmations. Pinchora then keeps one small record for 35 days (your ID and when you last collected rewards) so that deleting does not reset waiting times, and deletes it too. A server can remove Pinchora at any time from its settings in Discord.",
              )}{" "}
            </p>
          ),
        },
        {
          title: spaTranslate("Who else sees it"),
          body: (
            <p>
              {" "}
              {spaTranslate(
                "Pinchora does not sell data and shows nothing to advertisers. Your information is held by the companies that host Pinchora and, as the platform, by Discord. Other players see what a card shows, unless you made it private.",
              )}{" "}
            </p>
          ),
        },
        {
          title: spaTranslate("Questions and requests"),
          body: (
            <p>
              {" "}
              {spaTranslate("Ask in the")}{" "}
              <a href={SUPPORT_URL} rel="noopener noreferrer">
                {" "}
                {spaTranslate("support server")}{" "}
              </a>{" "}
              {spaTranslate(". See also the")} <Link to="/terms">{spaTranslate("terms of service")}</Link>.
            </p>
          ),
        },
      ]}
    />
  );
}

export function Terms() {
  return (
    <LegalPage
      title={spaTranslate("Terms of service")}
      intro={spaTranslate(
        "By adding Pinchora to a server or using its commands, website or dashboard you agree to these terms.",
      )}
      sections={[
        {
          title: spaTranslate("Using Pinchora"),
          body: (
            <p>
              {" "}
              {spaTranslate(
                "Follow Discord's Terms of Service and Community Guidelines. Server managers are responsible for how they set Pinchora up in their server and for telling their members what it does.",
              )}{" "}
            </p>
          ),
        },
        {
          title: spaTranslate("Shells, pearls and items"),
          body: (
            <p>
              {" "}
              {spaTranslate(
                "Shells, pearls, furniture, badges and every other item exist only inside the game. They have no value in money, cannot be bought or sold for money and may be changed, rebalanced or reset, for example to fix a bug or undo abuse.",
              )}{" "}
            </p>
          ),
        },
        {
          title: spaTranslate("Fair play"),
          body: (
            <ul>
              <li>
                {" "}
                {spaTranslate(
                  "Do not exploit bugs, run scripts or extra accounts to farm rewards, or interfere with other players.",
                )}{" "}
              </li>
              <li>
                {" "}
                {spaTranslate(
                  "Do not put harassment, hate or anything illegal in names, bios, notes, captions or suggestions.",
                )}{" "}
              </li>
              <li>{spaTranslate("We may remove content and limit or end access for anyone who does.")}</li>
            </ul>
          ),
        },
        {
          title: spaTranslate("Your content"),
          body: (
            <p>
              {" "}
              {spaTranslate(
                "You keep what you write. You allow Pinchora to store and show it as the feature you used intends, such as a bio on your card or a note in a guestbook.",
              )}{" "}
            </p>
          ),
        },
        {
          title: spaTranslate("Availability"),
          body: (
            <p>
              {" "}
              {spaTranslate(
                "Pinchora is provided as it is, without promises that it is always on or free of mistakes. Features can change or end. To the extent the law allows, we are not liable for loss of in-game items or for any indirect damage.",
              )}{" "}
            </p>
          ),
        },
        {
          title: "Ending",
          body: (
            <p>
              {" "}
              {spaTranslate(
                "You can stop at any time: remove the bot from your server, or delete your data with",
              )}{" "}
              <code>/privacy</code>
              {spaTranslate(". See the")} <Link to="/privacy">{spaTranslate("privacy policy")}</Link>{" "}
              {spaTranslate("for what that removes.")}{" "}
            </p>
          ),
        },
        {
          title: spaTranslate("Changes and contact"),
          body: (
            <p>
              {" "}
              {spaTranslate(
                "We may update these terms; the date above changes when we do. Questions go to the",
              )}{" "}
              <a href={SUPPORT_URL} rel="noopener noreferrer">
                {" "}
                {spaTranslate("support server")}{" "}
              </a>
              .
            </p>
          ),
        },
      ]}
    />
  );
}
