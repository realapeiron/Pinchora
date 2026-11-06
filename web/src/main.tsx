import { QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RouterProvider } from "react-router/dom";

import { ApiError, needsLogin } from "./lib/api";
import { router } from "./router";
import "./styles/tokens.css";
import "./styles/app.css";

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

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </StrictMode>,
);
