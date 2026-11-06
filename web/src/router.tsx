import { Navigate, createBrowserRouter } from "react-router";

import { GuildLayout } from "./pages/GuildLayout";
import { Login } from "./pages/Login";
import { NotFound } from "./pages/NotFound";
import { Overview } from "./pages/Overview";
import { RequireLogin } from "./pages/RequireLogin";
import { Servers } from "./pages/Servers";

// Admin pages live under /g/:guildId; later phases add their routes next to the index (Overview).
export const router = createBrowserRouter([
  { path: "/", element: <Navigate to="/servers" replace /> },
  { path: "/login", element: <Login /> },
  { path: "/servers", element: <RequireLogin>{(me) => <Servers me={me} />}</RequireLogin> },
  {
    path: "/g/:guildId",
    element: <RequireLogin>{(me) => <GuildLayout me={me} />}</RequireLogin>,
    children: [{ index: true, element: <Overview /> }],
  },
  { path: "*", element: <NotFound /> },
]);
