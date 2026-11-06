import { Navigate, createBrowserRouter } from "react-router";

import { AuditLog } from "./pages/AuditLog";
import { CommandEditor } from "./pages/CommandEditor";
import { Commands } from "./pages/Commands";
import { Community } from "./pages/Community";
import { GuildLayout } from "./pages/GuildLayout";
import { MyHouse, VisitHouse } from "./pages/House";
import { Leveling } from "./pages/Leveling";
import { Login } from "./pages/Login";
import { Modules } from "./pages/Modules";
import { NotFound } from "./pages/NotFound";
import { Overview } from "./pages/Overview";
import { Points } from "./pages/Points";
import { RequireLogin } from "./pages/RequireLogin";
import { ResponderEditor } from "./pages/ResponderEditor";
import { Responders } from "./pages/Responders";
import { Roles } from "./pages/Roles";
import { ServerLog } from "./pages/ServerLog";
import { Servers } from "./pages/Servers";
import { Welcome } from "./pages/Welcome";
import { Landing } from "./site/Landing";
import { SiteLayout } from "./site/SiteLayout";
import { SiteNotFound } from "./site/SiteNotFound";

// The public site (landing, commands, features, legal) sits under one layout, and the landing page is
// the front door for everyone; the dashboard keeps its own paths. The rarer pages load on demand.
// Admin pages live under /g/:guildId, one per sidebar entry (GuildLayout's NAV).
export const router = createBrowserRouter([
  {
    element: <SiteLayout />,
    children: [
      { path: "/", element: <Landing /> },
      {
        path: "/commands",
        lazy: async () => ({ Component: (await import("./site/Commands")).Commands }),
      },
      { path: "/features", element: <Navigate to="/#features" replace /> },
      {
        path: "/features/:area",
        lazy: async () => ({ Component: (await import("./site/Feature")).FeaturePage }),
      },
      {
        path: "/privacy",
        lazy: async () => ({ Component: (await import("./site/Legal")).Privacy }),
      },
      {
        path: "/terms",
        lazy: async () => ({ Component: (await import("./site/Legal")).Terms }),
      },
      { path: "*", element: <SiteNotFound /> },
    ],
  },
  { path: "/login", element: <Login /> },
  { path: "/servers", element: <RequireLogin>{(me) => <Servers me={me} />}</RequireLogin> },
  { path: "/me/house", element: <RequireLogin>{(me) => <MyHouse me={me} />}</RequireLogin> },
  { path: "/house/:userId", element: <RequireLogin>{(me) => <VisitHouse me={me} />}</RequireLogin> },
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
      { path: "commands", element: <Commands /> },
      { path: "commands/new", element: <CommandEditor /> },
      { path: "commands/:commandId", element: <CommandEditor /> },
      { path: "responders", element: <Responders /> },
      { path: "responders/new", element: <ResponderEditor /> },
      { path: "responders/:responderId", element: <ResponderEditor /> },
      { path: "roles", element: <Roles /> },
      { path: "audit", element: <AuditLog /> },
      { path: "*", element: <NotFound /> },
    ],
  },
]);
