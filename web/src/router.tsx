import { ModerationPage } from "./reef/ModerationPage";
import { Navigate, Outlet, createBrowserRouter, type RouteObject } from "react-router";
import { useLocale } from "./i18n";

import { GuildLayout } from "./pages/GuildLayout";
import { MyHouse, VisitHouse } from "./pages/House";
import { Login } from "./pages/Login";
import { NotFound } from "./pages/NotFound";
import { RequireLogin } from "./pages/RequireLogin";
import { Servers } from "./pages/Servers";
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
      { index: true, lazy: async () => ({ Component: (await import("./pages/Overview")).Overview }) },
      { path: "modules", lazy: async () => ({ Component: (await import("./pages/Modules")).Modules }) },
      { path: "access", lazy: async () => ({ Component: (await import("./pages/Access")).Access }) },
      { path: "welcome", lazy: async () => ({ Component: (await import("./pages/Welcome")).Welcome }) },
      {
        path: "branding",
        lazy: async () => ({ Component: (await import("./pages/Branding")).BrandingPage }),
      },
      {
        path: "welcome-image",
        lazy: async () => ({ Component: (await import("./pages/WelcomeImage")).WelcomeImage }),
      },
      {
        path: "message-studio",
        lazy: async () => ({ Component: (await import("./pages/MessageStudio")).MessageStudio }),
      },
      { path: "leveling", lazy: async () => ({ Component: (await import("./pages/Leveling")).Leveling }) },
      { path: "points", lazy: async () => ({ Component: (await import("./pages/Points")).Points }) },
      { path: "log", lazy: async () => ({ Component: (await import("./pages/ServerLog")).ServerLog }) },
      { path: "community", lazy: async () => ({ Component: (await import("./pages/Community")).Community }) },
      {
        path: "giveaways",
        lazy: async () => ({ Component: (await import("./pages/CommunityV2")).CommunityGiveaways }),
      },
      {
        path: "polls",
        lazy: async () => ({ Component: (await import("./pages/CommunityV2")).CommunityPolls }),
      },
      {
        path: "suggestions",
        lazy: async () => ({ Component: (await import("./pages/CommunityV2")).CommunitySuggestions }),
      },
      {
        path: "reminders",
        lazy: async () => ({ Component: (await import("./pages/CommunityV2")).CommunityReminders }),
      },
      {
        path: "highlights",
        lazy: async () => ({ Component: (await import("./pages/CommunityV2")).CommunityHighlights }),
      },
      {
        path: "auto-channels",
        lazy: async () => ({ Component: (await import("./pages/CommunityV2")).CommunityAutoChannels }),
      },
      {
        path: "anniversaries",
        lazy: async () => ({ Component: (await import("./pages/CommunityV2")).CommunityAnniversaries }),
      },
      {
        path: "confessions",
        lazy: async () => ({ Component: (await import("./pages/CommunityV2")).CommunityConfessions }),
      },
      {
        path: "community-pack",
        lazy: async () => ({ Component: (await import("./pages/CommunityPack")).CommunityPack }),
      },
      { path: "roblox", lazy: async () => ({ Component: (await import("./pages/Roblox")).Roblox }) },
      { path: "events", lazy: async () => ({ Component: (await import("./pages/Events")).Events }) },
      {
        path: "moderation",
        lazy: async () => ({ Component: (await import("./pages/Moderation")).Moderation }),
      },
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
      { path: "cases", lazy: async () => ({ Component: (await import("./pages/Cases")).Cases }) },
      { path: "staff-log", lazy: async () => ({ Component: (await import("./pages/StaffLog")).StaffLog }) },
      {
        path: "temp-voice",
        lazy: async () => ({ Component: (await import("./pages/TempVoice")).TempVoice }),
      },
      { path: "birthdays", lazy: async () => ({ Component: (await import("./pages/Birthdays")).Birthdays }) },
      { path: "commands", lazy: async () => ({ Component: (await import("./pages/Commands")).Commands }) },
      {
        path: "commands/new",
        lazy: async () => ({ Component: (await import("./pages/CommandEditor")).CommandEditor }),
      },
      {
        path: "commands/:commandId",
        lazy: async () => ({ Component: (await import("./pages/CommandEditor")).CommandEditor }),
      },
      {
        path: "responders",
        lazy: async () => ({ Component: (await import("./pages/Responders")).Responders }),
      },
      {
        path: "responders/new",
        lazy: async () => ({ Component: (await import("./pages/ResponderEditor")).ResponderEditor }),
      },
      {
        path: "responders/:responderId",
        lazy: async () => ({ Component: (await import("./pages/ResponderEditor")).ResponderEditor }),
      },
      { path: "roles", lazy: async () => ({ Component: (await import("./pages/Roles")).Roles }) },
      { path: "audit", lazy: async () => ({ Component: (await import("./pages/AuditLog")).AuditLog }) },
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
