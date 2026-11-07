import { translate as spaTranslate } from "../i18n";
import { Link } from "react-router";

import { Crab } from "../components/Crab";
import { number, t, translate } from "../i18n";
import { PixelLink } from "../components/PixelButton";
import { Amp } from "./Amp";
import { InviteButton } from "./Buttons";
import { FACTS, FAQ, STEPS } from "./content";
import { FEATURES } from "./features";
import { Gallery } from "./Gallery";
import { Shot } from "./Frame";
import { compact } from "./format";
import { PixelIcon } from "./PixelIcon";
import { useCountUp, useInView } from "./motion";
import { Reveal } from "./Reveal";
import { useLanding } from "./useLanding";

export function Landing() {
  return (
    <>
      <Hero />
      <Stats />
      <Reveal as="section" className="band" id="showcase">
        <div className="wrap">
          <SectionHead
            eyebrow={spaTranslate("SEE IT IN DISCORD")}
            title={spaTranslate("Cards drawn by the bot, not by a template")}
            lead={spaTranslate(
              "Every picture on this page was rendered by Pinchora's own code. This is exactly what lands in your channel.",
            )}
          />
          <Gallery />
        </div>
      </Reveal>
      <Features />
      <HowItWorks />
      <DashboardSpotlight />
      <Faq />
      <ClosingCall />
    </>
  );
}

function SectionHead({ eyebrow, title, lead }: { eyebrow: string; title: string; lead?: string }) {
  return (
    <div className="section-head">
      <p className="pixel-label eyebrow">{translate(eyebrow)}</p>
      <h2 className="section-title">{translate(title)}</h2>
      {lead && <p className="section-lead">{translate(lead)}</p>}
    </div>
  );
}

// /Hero

function Hero() {
  return (
    <section className="hero" aria-labelledby="hero-title">
      <div className="wrap hero-grid">
        <div className="hero-copy">
          <p className="pixel-label eyebrow">{t("A COZY PIXEL WORLD FOR DISCORD")}</p>
          <h1 id="hero-title" className="hero-title">
            {t("Make your server a cozy pixel world.")}
          </h1>
          <p className="hero-lead">
            {t(
              "Every member gets a house to decorate, a pet to raise, a shore to fish from and friends to play with. Levels, welcome cards, polls and giveaways keep the server itself running.",
            )}
          </p>
          <div className="hero-actions">
            <InviteButton className="hero-cta" />
            <PixelLink href="/servers" variant="secondary" className="hero-cta">
              {t("Open dashboard")}
            </PixelLink>
          </div>
          <p className="hero-note muted">
            {t("Slash commands only, nothing to install. Type /help and start.")}
          </p>
        </div>
        <div className="hero-stage">
          <Shot name="room" className="hero-room" eager />
          <Shot name="profile" className="hero-profile" eager />
          <span className="hero-sparkle hero-sparkle-a" aria-hidden="true" />
          <span className="hero-sparkle hero-sparkle-b" aria-hidden="true" />
          <span className="hero-sparkle hero-sparkle-c" aria-hidden="true" />
        </div>
      </div>
      <div className="sea" aria-hidden="true">
        <div className="sea-wave sea-wave-back" />
        <div className="sea-wave sea-wave-mid" />
        <div className="sea-crab">
          <Crab scale={5} />
        </div>
        <div className="sea-wave sea-wave-front" />
      </div>
    </section>
  );
}

// /Stats

function Stat({ value, label, run }: { value: number; label: string; run: boolean }) {
  const shown = useCountUp(value, run);
  return (
    <li className="stat">
      <span className="stat-value" aria-label={`${number(value)} ${translate(label)}`}>
        <span aria-hidden="true">{compact(shown)}</span>
      </span>
      <span className="pixel-label stat-label">{translate(label).toLocaleUpperCase()}</span>
    </li>
  );
}

function Stats() {
  const landing = useLanding();
  const stats = landing.data?.stats;
  const [ref, seen] = useInView<HTMLDivElement>();
  return (
    <section className="stats-band" aria-label={t("Pinchora in numbers")}>
      <div className="wrap" ref={ref}>
        {stats && (
          <ul className="stats">
            <Stat value={stats.servers} label={spaTranslate("servers")} run={seen} />
            <Stat value={stats.players} label={spaTranslate("players")} run={seen} />
            <Stat value={stats.houses} label={spaTranslate("houses built")} run={seen} />
            <Stat value={stats.pets} label={spaTranslate("pets adopted")} run={seen} />
          </ul>
        )}
        <ul className="facts">
          {FACTS.map((f) => (
            <li key={f.label}>
              <b>{number(f.value)}</b> {translate(f.label)}
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}

// /Features

function Features() {
  return (
    <Reveal as="section" className="band band-plain" id="features">
      <div className="wrap">
        <SectionHead
          eyebrow={spaTranslate("WHAT IT DOES")}
          title={spaTranslate("One bot, six corners of your server")}
          lead={spaTranslate(
            "Switch on what you like and leave the rest. Every part can be turned off for the server or for single channels.",
          )}
        />
        <ul className="feature-grid">
          {FEATURES.map((f) => (
            <li key={f.slug}>
              <Link to={`/features/${f.slug}`} className="feature-card panel">
                <span className="panel-body feature-card-body">
                  <PixelIcon name={f.icon} scale={5} className="feature-icon" />
                  <span className="pixel-title feature-title">
                    <Amp text={translate(f.title)} />
                  </span>
                  <span className="feature-blurb">{translate(f.blurb)}</span>
                  <span className="pixel-label feature-more">{t("LEARN MORE")}</span>
                </span>
              </Link>
            </li>
          ))}
        </ul>
      </div>
    </Reveal>
  );
}

// /How it works

function HowItWorks() {
  return (
    <Reveal as="section" className="band" id="how">
      <div className="wrap">
        <SectionHead
          eyebrow={spaTranslate("HOW IT WORKS")}
          title={spaTranslate("Up and running in three steps")}
        />
        <ol className="steps">
          {STEPS.map((step, i) => (
            <li key={step.title} className="step panel">
              <div className="panel-body step-body">
                <span className="step-number" aria-hidden="true">
                  {i + 1}
                </span>
                <h3 className="pixel-title">{translate(step.title)}</h3>
                <p>{translate(step.text)}</p>
                <p className="pixel-label step-detail">{translate(step.detail).toLocaleUpperCase()}</p>
              </div>
            </li>
          ))}
        </ol>
        <div className="steps-cta">
          <InviteButton />
          <Link to="/commands" className="steps-link">
            {t("Browse every command")}
          </Link>
        </div>
      </div>
    </Reveal>
  );
}

// /Dashboard

/** A picture of the dashboard's settings page made of the same parts, not a screenshot that goes stale. */
function DashboardSpotlight() {
  const feature = FEATURES.find((f) => f.slug === "web-dashboard");
  return (
    <Reveal as="section" className="band band-plain" id="dashboard">
      <div className="wrap spotlight">
        <div className="spotlight-copy">
          <p className="pixel-label eyebrow">{t("WEB DASHBOARD")}</p>
          <h2 className="section-title">{t("Settings on a page, with a live preview")}</h2>
          <p className="section-lead">{feature?.lead && translate(feature.lead)}</p>
          <ul className="ticks">
            {feature?.points.slice(0, 4).map((p) => (
              <li key={p.title}>
                <b>{translate(p.title)}.</b> {translate(p.text)}
              </li>
            ))}
          </ul>
          <div className="spotlight-actions">
            <PixelLink href="/servers">{t("Open dashboard")}</PixelLink>
            <Link to="/features/web-dashboard" className="steps-link">
              {t("About the dashboard")}
            </Link>
          </div>
        </div>
        <div className="mock panel" aria-hidden="true">
          <div className="mock-body panel-body">
            <div className="mock-bar">
              <span className="mock-dot" />
              <span className="mock-dot" />
              <span className="mock-dot" />
              <span className="pixel-label mock-url">{spaTranslate("pinchora / welcome")}</span>
            </div>
            <div className="mock-row">
              <div>
                <p className="pixel-title">{t("WELCOME CARDS")}</p>
                <p className="muted mock-small">{t("Greet new members in #welcome")}</p>
              </div>
              <span className="pixel-toggle is-on mock-toggle">
                <span className="pixel-toggle-knob" />
                <span className="pixel-toggle-text">{t("ON")}</span>
              </span>
            </div>
            <div className="mock-field">
              <p className="pixel-label muted">{t("MESSAGE")}</p>
              <p className="mock-input">
                {" "}
                {spaTranslate("Welcome to")} {"{server}"}, {"{user}"}
                {spaTranslate("! You are member number")} {"{members}"}.
              </p>
            </div>
            <div className="mock-preview">
              <Shot name="welcome" alt="" />
            </div>
          </div>
        </div>
      </div>
    </Reveal>
  );
}

// /FAQ

function Faq() {
  return (
    <Reveal as="section" className="band" id="faq">
      <div className="wrap wrap-narrow">
        <SectionHead eyebrow={spaTranslate("QUESTIONS")} title={spaTranslate("Before you add it")} />
        <div className="faq">
          {FAQ.map((item) => (
            <details key={item.question} className="faq-item panel">
              <summary className="faq-question">{translate(item.question)}</summary>
              <div className="faq-answer">
                <p>{translate(item.answer)}</p>
              </div>
            </details>
          ))}
        </div>
      </div>
    </Reveal>
  );
}

// /Closing call

function ClosingCall() {
  return (
    <Reveal as="section" className="band band-plain closing">
      <div className="wrap">
        <div className="panel panel-accent closing-panel">
          <div className="panel-body closing-body">
            <Crab scale={8} className="closing-crab" />
            <h2 className="section-title">{t("Ready to meet the crab?")}</h2>
            <p className="section-lead">
              {t("Add Pinchora to your server and give everyone a house to come home to.")}
            </p>
            <div className="hero-actions closing-actions">
              <InviteButton className="hero-cta" />
              <PixelLink href="/servers" variant="secondary" className="hero-cta">
                {t("Open dashboard")}
              </PixelLink>
            </div>
          </div>
        </div>
      </div>
    </Reveal>
  );
}
