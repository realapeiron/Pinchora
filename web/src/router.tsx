import { ModerationPage } from "./reef/ModerationPage";
import { Navigate, Outlet, createBrowserRouter, type RouteObject } from "react-router";
import { useLocale } from "./i18n";

import { AuditLog } from "./pages/AuditLog";
import { Cases } from "./pages/Cases";
import { CommandEditor } from "./pages/CommandEditor";
import { Commands } from "./pages/Commands";
import { Community } from "./pages/Community";
import { GuildLayout } from "./pages/GuildLayout";
import { MyHouse, VisitHouse } from "./pages/House";
import { Leveling } from "./pages/Leveling";
import { Login } from "./pages/Login";
import { Moderation } from "./pages/Moderation";
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
import { StaffLog } from "./pages/StaffLog";
import { TempVoice } from "./pages/TempVoice";
import { Welcome } from "./pages/Welcome";
import { WelcomeImage } from "./pages/WelcomeImage";
import { Landing } from "./site/Landing";
import { SiteLayout } from "./site/SiteLayout";
import { SiteNotFound } from "./site/SiteNotFound";
import { StatusPage } from "./site/Status";

// The public site (landing, commands, features, legal) sits under one layout, and the landing page is
// the front door for everyone; the dashboard keeps its own paths. The rarer pages load on demand.
// Admin pages live under /g/:guildId, one per sidebar entry (GuildLayout's NAV).
const localizedRoutes: RouteObject[] = [
  {
    element: <SiteLayout />,
    children: [
      { path: "/", element: <Landing /> },
      { path: "/status", element: <StatusPage /> },
      {
        path: "/lb/:slug",
        lazy: async () => ({ Component: (await import("./site/Leaderboard")).PublicLeaderboard }),
      },
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
      { path: "/stats", lazy: async () => ({ Component: (await import("./site/Stats")).GlobalStatsPage }) },
      {
        path: "/stats/:guildId",
        lazy: async () => ({ Component: (await import("./site/Stats")).PublicServerStats }),
      },
      { path: "*", element: <SiteNotFound /> },
    ],
  },
  { path: "/verify/:token", lazy: async () => ({ Component: (await import("./pages/Verify")).Verify }) },
  {
    path: "/appeals/:guild/:caseNumber",
    lazy: async () => ({ Component: (await import("./pages/ModParity")).AppealForm }),
  },
  { path: "/appeals", lazy: async () => ({ Component: (await import("./pages/ModParity")).AppealForm }) },
  { path: "/login", element: <Login /> },
  { path: "/servers", element: <RequireLogin>{(me) => <Servers me={me} />}</RequireLogin> },
  {
    path: "/feeds/my-channels",
    lazy: async () => ({ Component: (await import("./pages/Feeds")).MyFeedsChannelsRoute }),
  },
  { path: "/reef/moderation", element: <RequireLogin>{() => <ModerationPage />}</RequireLogin> },
  { path: "/me/house", element: <RequireLogin>{(me) => <MyHouse me={me} />}</RequireLogin> },
  { path: "/house/:userId", element: <RequireLogin>{(me) => <VisitHouse me={me} />}</RequireLogin> },
  {
    path: "/g/:guildId",
    element: <RequireLogin>{(me) => <GuildLayout me={me} />}</RequireLogin>,
    children: [
      { index: true, element: <Overview /> },
      { path: "modules", element: <Modules /> },
      { path: "welcome", element: <Welcome /> },
      {
        path: "branding",
        lazy: async () => ({ Component: (await import("./pages/Branding")).BrandingPage }),
      },
      { path: "welcome-image", element: <WelcomeImage /> },
      { path: "leveling", element: <Leveling /> },
      { path: "points", element: <Points /> },
      { path: "log", element: <ServerLog /> },
      { path: "community", element: <Community /> },
      {
        path: "community-pack",
        lazy: async () => ({ Component: (await import("./pages/CommunityPack")).CommunityPack }),
      },
      { path: "roblox", lazy: async () => ({ Component: (await import("./pages/Roblox")).Roblox }) },
      { path: "events", lazy: async () => ({ Component: (await import("./pages/Events")).Events }) },
      { path: "moderation", element: <Moderation /> },
      {
        path: "mod-parity",
        lazy: async () => ({ Component: (await import("./pages/ModParity")).ModParity }),
      },
      { path: "safety", lazy: async () => ({ Component: (await import("./pages/Safety")).Safety }) },
      { path: "anti-nuke", lazy: async () => ({ Component: (await import("./pages/AntiNuke")).AntiNuke }) },
      {
        path: "command-control",
        lazy: async () => ({ Component: (await import("./pages/CommandControl")).CommandControl }),
      },
      {
        path: "backup",
        lazy: async () => ({ Component: (await import("./pages/ConfigBackup")).ConfigBackup }),
      },
      { path: "automod", lazy: async () => ({ Component: (await import("./pages/AutoMod")).AutoMod }) },
      { path: "cases", element: <Cases /> },
      { path: "staff-log", element: <StaffLog /> },
      { path: "temp-voice", element: <TempVoice /> },
      { path: "birthdays", lazy: async () => ({ Component: (await import("./pages/Birthdays")).Birthdays }) },
      { path: "commands", element: <Commands /> },
      { path: "commands/new", element: <CommandEditor /> },
      { path: "commands/:commandId", element: <CommandEditor /> },
      { path: "responders", element: <Responders /> },
      { path: "responders/new", element: <ResponderEditor /> },
      { path: "responders/:responderId", element: <ResponderEditor /> },
      { path: "roles", element: <Roles /> },
      { path: "audit", element: <AuditLog /> },
      { path: "starboard", lazy: async () => ({ Component: (await import("./pages/Starboard")).Starboard }) },
      { path: "invites", lazy: async () => ({ Component: (await import("./pages/Invites")).Invites }) },
      { path: "stats", lazy: async () => ({ Component: (await import("./pages/Stats")).Stats }) },
      { path: "utility", lazy: async () => ({ Component: (await import("./pages/Utility")).Utility }) },
      {
        path: "achievements",
        lazy: async () => ({ Component: (await import("./pages/Achievements")).Achievements }),
      },
      {
        path: "tickets/:ticketId?",
        lazy: async () => ({ Component: (await import("./pages/Tickets")).Tickets }),
      },
      { path: "feeds/*", lazy: async () => ({ Component: (await import("./pages/Feeds")).Feeds }) },
      { path: "trackers", lazy: async () => ({ Component: (await import("./pages/Trackers")).Trackers }) },
      { path: "*", element: <NotFound /> },
    ],
  },
];

// Legacy pages translate on mount. Keep their locale boundary below the router so
// player studios can refresh their catalogs without losing unsaved editor state.
function LocalizedPages() {
  const language = useLocale();
  return <Outlet key={language} />;
}

export const router = createBrowserRouter([
  { Component: LocalizedPages, children: localizedRoutes },
  {
    path: "/me/*",
    lazy: async () => ({ Component: (await import("./pages/player/Player")).PlayerRoutes }),
  },
]);
