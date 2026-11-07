// Keeps the tab title and the tags in step with the page while the app changes pages without a reload.

import { DASHBOARD_TITLE, isAppPath, publicRoute } from "./routes";
import { translate } from "../i18n";

/** What the router offers: where it is, and a way to hear about changes. */
interface Watchable {
  state: { location: { pathname: string } };
  subscribe(listener: (state: { location: { pathname: string } }) => void): () => void;
}

function setContent(selector: string, value: string): void {
  document.head.querySelector(selector)?.setAttribute("content", value);
}

function setRobots(value: string | null): void {
  let tag = document.head.querySelector<HTMLMetaElement>('meta[name="robots"]');
  if (value === null) {
    tag?.remove();
    return;
  }
  if (!tag) {
    tag = document.createElement("meta");
    tag.name = "robots";
    document.head.append(tag);
  }
  tag.content = value;
}

export function refreshMeta(pathname: string): void {
  const route = publicRoute(pathname);
  const title = route?.title ?? (isAppPath(pathname) ? DASHBOARD_TITLE : "Page not found | Pinchora");
  document.title = translate(title);
  if (route) {
    setContent('meta[name="description"]', translate(route.description));
    setContent('meta[property="og:title"]', translate(title));
    setContent('meta[property="og:description"]', translate(route.description));
    setContent('meta[name="twitter:title"]', translate(title));
    setContent('meta[name="twitter:description"]', translate(route.description));
    document.head
      .querySelector('link[rel="canonical"]')
      ?.setAttribute("href", `${window.location.origin}${route.path}`);
    setRobots(null);
  } else {
    setRobots("noindex, nofollow");
  }
}

/** Applies the title and tags of the current page and of every page the router moves to. */
export function watchMeta(router: Watchable): () => void {
  refreshMeta(router.state.location.pathname);
  return router.subscribe((state) => refreshMeta(state.location.pathname));
}
