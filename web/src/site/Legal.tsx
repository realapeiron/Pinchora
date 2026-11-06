import type { ReactNode } from "react";
import { Link } from "react-router";

import { SUPPORT_URL } from "./content";

/** When both pages were last written; change it with the words. */
const UPDATED = "6 November 2026";

interface Section {
  title: string;
  body: ReactNode;
}

function LegalPage({ title, intro, sections }: { title: string; intro: string; sections: Section[] }) {
  return (
    <div className="wrap wrap-narrow legal">
      <header className="page-intro">
        <p className="pixel-label eyebrow">LEGAL</p>
        <h1 className="section-title">{title}</h1>
        <p className="section-lead">{intro}</p>
        <p className="muted legal-updated">
          Last updated {UPDATED}. This is a plain-language draft that will be reviewed before launch.
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
      title="Privacy policy"
      intro="Pinchora is a game and a toolbox inside Discord. This page says what it keeps about you and your server, and how to get rid of it."
      sections={[
        {
          title: "What Pinchora stores about players",
          body: (
            <>
              <p>Your Discord user ID, and what you do with the bot:</p>
              <ul>
                <li>experience points, levels, shells and other currencies, streaks and reward timers;</li>
                <li>your bio, card style, character, badges and titles;</li>
                <li>your house and furniture, pets, fish book, friends and partner;</li>
                <li>
                  game results and statistics, reminders you set, notes you sign in a guestbook, suggestions,
                  polls and giveaway entries you make.
                </li>
              </ul>
              <p>
                Pinchora counts messages to give chat XP. It does not store the text of what people write in
                chat. Names and avatars are read from Discord when a card is drawn.
              </p>
            </>
          ),
        },
        {
          title: "What Pinchora stores about servers",
          body: (
            <p>
              The server ID, the settings its managers choose (channels, roles, XP rules, the store, which
              parts are switched on) and a log of changes made in the dashboard or with commands: who changed
              what, with the values before and after. Log entries are deleted after 180 days.
            </p>
          ),
        },
        {
          title: "The website and the dashboard",
          body: (
            <>
              <p>
                The public pages set no cookies and load no third-party scripts, trackers or advertising.
                Fonts and pictures come from this site (once you log in, your avatar comes from Discord).
              </p>
              <p>
                When you log in to the dashboard with Discord, Pinchora receives your user ID, username,
                avatar and the list of servers you are in, so it can show the ones you manage. They are kept
                in a session on Pinchora&apos;s servers behind one cookie that the page&apos;s scripts cannot
                read. Discord&apos;s access tokens never reach your browser. A session ends after seven days
                without use, or after thirty days.
              </p>
            </>
          ),
        },
        {
          title: "How your choices work",
          body: (
            <p>
              The <code>/privacy</code> command makes your profile private, leaves you off the global boards
              and hides your house. <b>Delete my data</b> in the same panel removes every row about you after
              two confirmations. Pinchora then keeps one small record for 35 days (your ID and when you last
              collected rewards) so that deleting does not reset waiting times, and deletes it too. A server
              can remove Pinchora at any time from its settings in Discord.
            </p>
          ),
        },
        {
          title: "Who else sees it",
          body: (
            <p>
              Pinchora does not sell data and shows nothing to advertisers. Your information is held by the
              companies that host Pinchora and, as the platform, by Discord. Other players see what a card
              shows, unless you made it private.
            </p>
          ),
        },
        {
          title: "Questions and requests",
          body: (
            <p>
              Ask in the{" "}
              <a href={SUPPORT_URL} rel="noopener noreferrer">
                support server
              </a>
              . See also the <Link to="/terms">terms of service</Link>.
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
      title="Terms of service"
      intro="By adding Pinchora to a server or using its commands, website or dashboard you agree to these terms."
      sections={[
        {
          title: "Using Pinchora",
          body: (
            <p>
              Follow Discord&apos;s Terms of Service and Community Guidelines. Server managers are responsible
              for how they set Pinchora up in their server and for telling their members what it does.
            </p>
          ),
        },
        {
          title: "Shells, pearls and items",
          body: (
            <p>
              Shells, pearls, furniture, badges and every other item exist only inside the game. They have no
              value in money, cannot be bought or sold for money and may be changed, rebalanced or reset, for
              example to fix a bug or undo abuse.
            </p>
          ),
        },
        {
          title: "Fair play",
          body: (
            <ul>
              <li>
                Do not exploit bugs, run scripts or extra accounts to farm rewards, or interfere with other
                players.
              </li>
              <li>
                Do not put harassment, hate or anything illegal in names, bios, notes, captions or
                suggestions.
              </li>
              <li>We may remove content and limit or end access for anyone who does.</li>
            </ul>
          ),
        },
        {
          title: "Your content",
          body: (
            <p>
              You keep what you write. You allow Pinchora to store and show it as the feature you used
              intends, such as a bio on your card or a note in a guestbook.
            </p>
          ),
        },
        {
          title: "Availability",
          body: (
            <p>
              Pinchora is provided as it is, without promises that it is always on or free of mistakes.
              Features can change or end. To the extent the law allows, we are not liable for loss of in-game
              items or for any indirect damage.
            </p>
          ),
        },
        {
          title: "Ending",
          body: (
            <p>
              You can stop at any time: remove the bot from your server, or delete your data with{" "}
              <code>/privacy</code>. See the <Link to="/privacy">privacy policy</Link> for what that removes.
            </p>
          ),
        },
        {
          title: "Changes and contact",
          body: (
            <p>
              We may update these terms; the date above changes when we do. Questions go to the{" "}
              <a href={SUPPORT_URL} rel="noopener noreferrer">
                support server
              </a>
              .
            </p>
          ),
        },
      ]}
    />
  );
}
