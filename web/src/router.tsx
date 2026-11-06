import { Navigate, createBrowserRouter } from "react-router";

import { AuditLog } from "./pages/AuditLog";
import { Community } from "./pages/Community";
import { GuildLayout } from "./pages/GuildLayout";
import { Leveling } from "./pages/Leveling";
import { Login } from "./pages/Login";
import { Modules } from "./pages/Modules";
import { NotFound } from "./pages/NotFound";
import { Overview } from "./pages/Overview";
import { Points } from "./pages/Points";
import { RequireLogin } from "./pages/RequireLogin";
import { ServerLog } from "./pages/ServerLog";
import { Servers } from "./pages/Servers";
import { Welcome } from "./pages/Welcome";

// Admin pages live under /g/:guildId, one per sidebar entry (GuildLayout's NAV).
export const router = createBrowserRouter([
  { path: "/", element: <Navigate to="/servers" replace /> },
  { path: "/login", element: <Login /> },
  { path: "/servers", element: <RequireLogin>{(me) => <Servers me={me} />}</RequireLogin> },
  {
    path: "/g/:guildId",
    element: <RequireLogin>{(me) => <GuildLayout me={me} />}</RequireLogin>,
    children: [
      { index: true, element: <Overview /> },
      { path: "modules", element: <Modules /> },
      { path: "welcome", element: <Welcome /> },
      { path: "leveling", element: <Leveling /> },
      { path: "points", element: <Points /> },
      { path: "log", element: <ServerLog /> },
      { path: "community", element: <Community /> },
      { path: "audit", element: <AuditLog /> },
      { path: "*", element: <NotFound /> },
    ],
  },
  { path: "*", element: <NotFound /> },
]);
