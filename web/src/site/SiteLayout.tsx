import { useEffect, useState } from "react";
import { Link, NavLink, Outlet, ScrollRestoration, useLocation } from "react-router";

import { Crab } from "../components/Crab";
import { PixelLink } from "../components/PixelButton";
import { InviteButton } from "./Buttons";
import { DOCS_URL, GITHUB_URL, SUPPORT_URL } from "./content";
import { FEATURES } from "./features";
import { useLanding } from "./useLanding";

/** The header, the footer and the room between them, for every page of the public site. */
export function SiteLayout() {
  const [open, setOpen] = useState(false);
  const location = useLocation();
  const landing = useLanding();
  const viewer = landing.data?.viewer;

  // A page change closes the phone menu.
  useEffect(() => setOpen(false), [location.pathname, location.hash]);

  return (
    <div className="site">
      <a className="skip-link" href="#main">
        Skip to content
      </a>
      <header className="site-nav">
        <div className="site-nav-inner">
          <Link to="/" className="brand" aria-label="Pinchora home">
            <Crab scale={3} />
            <span className="brand-name">PINCHORA</span>
          </Link>
          <button
            type="button"
            className="site-nav-toggle"
            aria-expanded={open}
            aria-controls="site-menu"
            onClick={() => setOpen((o) => !o)}
          >
            <span className="site-nav-toggle-bars" aria-hidden="true" />
            <span className="visually-hidden">{open ? "Close menu" : "Open menu"}</span>
          </button>
          <nav id="site-menu" className={open ? "site-menu is-open" : "site-menu"} aria-label="Main">
            <ul className="site-links">
              <li>
                <Link to="/#features" className="site-link">
                  Features
                </Link>
              </li>
              <li>
                <Link to="/#how" className="site-link">
                  How it works
                </Link>
              </li>
              <li>
                <NavLink to="/commands" className="site-link">
                  Commands
                </NavLink>
              </li>
              <li>
                <Link to="/#faq" className="site-link">
                  FAQ
                </Link>
              </li>
            </ul>
            <div className="site-actions">
              {viewer ? (
                <PixelLink href="/servers" variant="secondary" className="site-dashboard">
                  <img className="avatar" src={viewer.avatar_url} alt="" width={24} height={24} />
                  Dashboard
                </PixelLink>
              ) : (
                <PixelLink href="/servers" variant="ghost" className="site-dashboard">
                  Log in
                </PixelLink>
              )}
              <InviteButton />
            </div>
          </nav>
        </div>
      </header>

      <main id="main" tabIndex={-1} className="site-main">
        <Outlet />
      </main>

      <footer className="site-footer">
        <div className="site-footer-inner">
          <div className="footer-brand">
            <Link to="/" className="brand" aria-label="Pinchora home">
              <Crab scale={4} />
              <span className="brand-name">PINCHORA</span>
            </Link>
            <p className="muted">A cozy pixel world for Discord servers.</p>
          </div>
          <nav className="footer-column" aria-label="Features">
            <h2 className="pixel-label footer-title">FEATURES</h2>
            <ul>
              {FEATURES.map((f) => (
                <li key={f.slug}>
                  <Link to={`/features/${f.slug}`}>{f.title}</Link>
                </li>
              ))}
            </ul>
          </nav>
          <nav className="footer-column" aria-label="Product">
            <h2 className="pixel-label footer-title">PINCHORA</h2>
            <ul>
              <li>
                <a href="/invite">Add to Discord</a>
              </li>
              <li>
                <Link to="/servers">Dashboard</Link>
              </li>
              <li>
                <Link to="/commands">Commands</Link>
              </li>
              <li>
                <Link to="/status">Status</Link>
              </li>
              <li>
                <Link to="/#faq">FAQ</Link>
              </li>
            </ul>
          </nav>
          <nav className="footer-column" aria-label="Community and legal">
            <h2 className="pixel-label footer-title">MORE</h2>
            <ul>
              <li>
                <a href={SUPPORT_URL} rel="noopener noreferrer">
                  Support server
                </a>
              </li>
              <li>
                <a href={GITHUB_URL} rel="noopener noreferrer">
                  GitHub
                </a>
              </li>
              <li>
                <a href={DOCS_URL} rel="noopener noreferrer">
                  Docs
                </a>
              </li>
              <li>
                <Link to="/terms">Terms</Link>
              </li>
              <li>
                <Link to="/privacy">Privacy</Link>
              </li>
            </ul>
          </nav>
        </div>
        <p className="site-footer-legal muted">
          © {new Date().getFullYear()} Pinchora. Pinchora is not affiliated with or endorsed by Discord Inc.
        </p>
      </footer>
      <ScrollRestoration />
    </div>
  );
}
