import { Link } from "react-router";

import { Crab } from "../components/Crab";
import { PixelLink } from "../components/PixelButton";
import { t } from "../i18n";

/** A page of the public site that is not there. */
export function SiteNotFound() {
  return (
    <div className="wrap not-found">
      <Crab scale={10} className="not-found-crab" />
      <p className="pixel-label eyebrow">{t("ERROR 404")}</p>
      <h1 className="section-title">{t("Lost at sea")}</h1>
      <p className="section-lead">{t("There is no page here. The tide may have taken it.")}</p>
      <div className="hero-actions">
        <PixelLink href="/">{t("Back to the shore")}</PixelLink>
        <PixelLink href="/commands" variant="secondary">
          {t("Browse commands")}
        </PixelLink>
      </div>
      <p className="muted">
        {t("Looking for your servers?")} <Link to="/servers">{t("Open the dashboard")}</Link>.
      </p>
    </div>
  );
}
