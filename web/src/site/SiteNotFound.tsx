import { Link } from "react-router";

import { Crab } from "../components/Crab";
import { PixelLink } from "../components/PixelButton";

/** A page of the public site that is not there. */
export function SiteNotFound() {
  return (
    <div className="wrap not-found">
      <Crab scale={10} className="not-found-crab" />
      <p className="pixel-label eyebrow">ERROR 404</p>
      <h1 className="section-title">Lost at sea</h1>
      <p className="section-lead">There is no page here. The tide may have taken it.</p>
      <div className="hero-actions">
        <PixelLink href="/">Back to the shore</PixelLink>
        <PixelLink href="/commands" variant="secondary">
          Browse commands
        </PixelLink>
      </div>
      <p className="muted">
        Looking for your servers? <Link to="/servers">Open the dashboard</Link>.
      </p>
    </div>
  );
}
