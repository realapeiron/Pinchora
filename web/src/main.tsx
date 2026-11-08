import { QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode, useEffect } from "react";
import { useLocale } from "./i18n";
import { createRoot } from "react-dom/client";
import { RouterProvider } from "react-router/dom";

import { ApiError, needsLogin } from "./lib/api";
import { router } from "./router";
import { refreshMeta, watchMeta } from "./site/meta";
import "./styles/tokens.css";
import "./styles/app.css";
import "./styles/site.css";

const queryClient = new QueryClient({
  queryCache: new QueryCache({
    // A session that runs out mid-visit: back to the login, then to where the user was.
    onError: (error, query) => {
      if (needsLogin(error) && query.queryKey[0] !== "/api/me") {
        const next = encodeURIComponent(window.location.pathname);
        window.location.assign(`/login?error=expired&next=${next}`);
      }
    },
  }),
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      staleTime: 15_000,
      retry: (count, error) => !(error instanceof ApiError && error.status < 500) && count < 2,
    },
  },
});

// Play starts with system body text so the two 600KB TTFs do not spend the cold art budget.
const playFonts = () => {
  const location = router.state.location;
  document.documentElement.dataset.reefPlay = String(
    /^\/(me\/house|house\/[^/]+)$/.test(location.pathname) &&
      new URLSearchParams(location.search).get("play") === "1",
  );
};
playFonts();
router.subscribe(playFonts);

// The tab title and tags follow the page the router is on.
watchMeta(router);

function LocalizedRouter() {
  const language = useLocale();
  useEffect(() => {
    document.documentElement.lang = language;
    refreshMeta(router.state.location.pathname);
  }, [language]);
  return <RouterProvider router={router} />;
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <LocalizedRouter />
    </QueryClientProvider>
  </StrictMode>,
);
