import { translate as spaTranslate } from "../i18n";
import { Link, useParams } from "react-router";

import { PixelLink } from "../components/PixelButton";
import { Amp } from "./Amp";
import { InviteButton } from "./Buttons";
import { FEATURES, featureBySlug } from "./features";
import { PixelIcon } from "./PixelIcon";
import { Shot } from "./Frame";
import { SiteNotFound } from "./SiteNotFound";

/** One area of Pinchora in full: what it does, a few pictures, and the commands to try first. */
export function FeaturePage() {
  const { area } = useParams();
  const feature = featureBySlug(area);
  if (!feature) return <SiteNotFound />;
  const others = FEATURES.filter((f) => f.slug !== feature.slug);
  return (
    <div className="wrap feature-page">
      <nav className="crumbs" aria-label="Breadcrumb">
        <Link to="/">Home</Link>
        <span aria-hidden="true"> / </span>
        <Link to="/#features">{spaTranslate("Features")}</Link>
        <span aria-hidden="true"> / </span>
        <span aria-current="page">{feature.title}</span>
      </nav>

      <header className="feature-hero">
        <PixelIcon name={feature.icon} scale={8} className="feature-hero-icon" />
        <div>
          <h1 className="section-title">
            <Amp text={feature.title} />
          </h1>
          <p className="section-lead">{feature.lead}</p>
          <div className="hero-actions">
            <InviteButton />
            <PixelLink href="/servers" variant="secondary">
              {spaTranslate("Open dashboard")}
            </PixelLink>
          </div>
        </div>
      </header>

      <ul className="point-grid">
        {feature.points.map((p) => (
          <li key={p.title} className="point panel">
            <div className="panel-body">
              <h2 className="pixel-title point-title">{p.title}</h2>
              <p>{p.text}</p>
            </div>
          </li>
        ))}
      </ul>

      <section aria-labelledby="shots-title" className="feature-shots">
        <h2 id="shots-title" className="pixel-title">
          WHAT IT LOOKS LIKE
        </h2>
        <div className="shot-grid">
          {feature.shots.map((s) => (
            <figure key={s.key} className="shot-figure">
              <Shot name={s.key} alt={s.alt} />
              <figcaption className="muted">{s.caption}</figcaption>
            </figure>
          ))}
        </div>
      </section>

      <section aria-labelledby="try-title" className="feature-try">
        <h2 id="try-title" className="pixel-title">
          TRY THESE FIRST
        </h2>
        <ul className="try-list">
          {feature.try.map((command) => (
            <li key={command}>
              {command.startsWith("/") ? (
                <Link to={`/commands?q=${encodeURIComponent(command.slice(1))}`} className="try-chip">
                  <code>{command}</code>
                </Link>
              ) : (
                <Link to="/servers" className="try-chip">
                  {command}
                </Link>
              )}
            </li>
          ))}
        </ul>
      </section>

      <section aria-labelledby="more-title" className="feature-more-areas">
        <h2 id="more-title" className="pixel-title">
          MORE FROM PINCHORA
        </h2>
        <ul className="more-list">
          {others.map((f) => (
            <li key={f.slug}>
              <Link to={`/features/${f.slug}`} className="more-link">
                <PixelIcon name={f.icon} scale={3} />
                {f.title}
              </Link>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
