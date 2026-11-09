import { lazy, Suspense } from "react";
const ModerationPage = lazy(() =>
  import("./reef/ModerationPage").then((m) => ({ default: m.ModerationPage })),
);
import { Navigate, Outlet, createBrowserRouter, type RouteObject } from "react-router";
import { useLocale } from "./i18n";
import { LegacyGuildRedirect } from "./features/shell/pages/LegacyGuildRedirect";

const GuildLayout = lazy(() =>
  import("./features/admin/pages/GuildLayout").then((m) => ({ default: m.GuildLayout })),
);
import { Login } from "./features/shell/pages/Login";
import { Loading } from "./components/States";
import { NotFound } from "./components/NotFound";
import { RequireLogin } from "./components/RequireLogin";
import { Servers } from "./features/shell/pages/Servers";
import { SiteLayout } from "./site/SiteLayout";
import { SiteNotFound } from "./site/SiteNotFound";

// The public site (landing, commands, features, legal) sits under one layout, and the landing page is
// the front door for everyone; the dashboard keeps its own paths. The rarer pages load on demand.
// Admin pages live under /g/:guildId, one per sidebar entry (GuildLayout's NAV).
const localizedRoutes: RouteObject[] = [
  {
    element: <SiteLayout />,
    children: [
      { path: "/", lazy: async () => ({ Component: (await import("./site/Landing")).Landing }) },
      { path: "/status", lazy: async () => ({ Component: (await import("./site/Status")).StatusPage }) },
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
  {
    path: "/verify/:token",
    lazy: async () => ({ Component: (await import("./features/moderation/pages/Verify")).Verify }),
  },
  {
    path: "/appeals/:guild/:caseNumber",
    lazy: async () => ({
      Component: (await import("./features/moderation/pages/ModerationDesk")).AppealForm,
    }),
  },
  {
    path: "/appeals",
    lazy: async () => ({
      Component: (await import("./features/moderation/pages/ModerationDesk")).AppealForm,
    }),
  },
  { path: "/login", element: <Login /> },
  { path: "/servers", element: <RequireLogin>{(me) => <Servers me={me} />}</RequireLogin> },
  {
    path: "/feeds/my-channels",
    lazy: async () => ({
      Component: (await import("./features/integrations/pages/Feeds")).MyFeedsChannelsRoute,
    }),
  },
  {
    path: "/reef/moderation",
    element: (
      <RequireLogin>
        {() => (
          <Suspense fallback={<Loading full />}>
            <ModerationPage />
          </Suspense>
        )}
      </RequireLogin>
    ),
  },
  {
    path: "/me/house",
    lazy: async () => {
      const { MyHouse } = await import("./features/house/pages/House");
      return { Component: () => <RequireLogin>{(me) => <MyHouse me={me} />}</RequireLogin> };
    },
  },
  {
    path: "/house/:userId",
    lazy: async () => {
      const { VisitHouse } = await import("./features/house/pages/House");
      return { Component: () => <RequireLogin>{(me) => <VisitHouse me={me} />}</RequireLogin> };
    },
  },
  {
    path: "/g/:guildId",
    element: (
      <RequireLogin>
        {(me) => (
          <Suspense fallback={<Loading full />}>
            <GuildLayout me={me} />
          </Suspense>
        )}
      </RequireLogin>
    ),
    children: [
      {
        index: true,
        lazy: async () => ({ Component: (await import("./features/admin/pages/Overview")).Overview }),
      },
      {
        path: "modules",
        lazy: async () => ({ Component: (await import("./features/admin/pages/Modules")).Modules }),
      },
      {
        path: "access",
        lazy: async () => ({ Component: (await import("./features/admin/pages/Access")).Access }),
      },
      {
        path: "welcome",
        lazy: async () => ({ Component: (await import("./features/admin/pages/Welcome")).Welcome }),
      },
      {
        path: "branding",
        lazy: async () => ({ Component: (await import("./features/admin/pages/Branding")).BrandingPage }),
      },
      {
        path: "welcome-image",
        lazy: async () => ({ Component: (await import("./features/admin/pages/WelcomeImage")).WelcomeImage }),
      },
      {
        path: "message-studio",
        lazy: async () => ({
          Component: (await import("./features/admin/pages/MessageStudio")).MessageStudio,
        }),
      },
      {
        path: "leveling",
        lazy: async () => ({ Component: (await import("./features/progression/pages/Leveling")).Leveling }),
      },
      {
        path: "points",
        lazy: async () => ({ Component: (await import("./features/economy/pages/Points")).Points }),
      },
      {
        path: "economy",
        lazy: async () => ({
          Component: (await import("./features/economy/pages/EconomySettings")).EconomySettings,
        }),
      },
      {
        path: "log",
        lazy: async () => ({ Component: (await import("./features/moderation/pages/ServerLog")).ServerLog }),
      },
      {
        path: "community",
        lazy: async () => ({ Component: (await import("./features/community/pages/Community")).Community }),
      },
      {
        path: "giveaways",
        lazy: async () => ({
          Component: (await import("./features/community/pages/CommunityTools")).CommunityGiveaways,
        }),
      },
      {
        path: "polls",
        lazy: async () => ({
          Component: (await import("./features/community/pages/CommunityTools")).CommunityPolls,
        }),
      },
      {
        path: "suggestions",
        lazy: async () => ({
          Component: (await import("./features/community/pages/CommunityTools")).CommunitySuggestions,
        }),
      },
      {
        path: "reminders",
        lazy: async () => ({
          Component: (await import("./features/community/pages/CommunityTools")).CommunityReminders,
        }),
      },
      {
        path: "highlights",
        lazy: async () => ({
          Component: (await import("./features/community/pages/CommunityTools")).CommunityHighlights,
        }),
      },
      {
        path: "auto-channels",
        lazy: async () => ({
          Component: (await import("./features/community/pages/CommunityTools")).CommunityAutoChannels,
        }),
      },
      {
        path: "anniversaries",
        lazy: async () => ({
          Component: (await import("./features/community/pages/CommunityTools")).CommunityAnniversaries,
        }),
      },
      {
        path: "confessions",
        lazy: async () => ({
          Component: (await import("./features/community/pages/CommunityTools")).CommunityConfessions,
        }),
      },
      { path: "community-pack", element: <LegacyGuildRedirect target="community-engagement" /> },
      {
        path: "community-engagement",
        lazy: async () => ({
          Component: (await import("./features/community/pages/CommunityEngagement")).CommunityEngagement,
        }),
      },
      {
        path: "roblox",
        lazy: async () => ({ Component: (await import("./features/integrations/pages/Roblox")).Roblox }),
      },
      {
        path: "events",
        lazy: async () => ({ Component: (await import("./features/community/pages/Events")).Events }),
      },
      {
        path: "moderation",
        lazy: async () => ({
          Component: (await import("./features/moderation/pages/Moderation")).Moderation,
        }),
      },
      { path: "mod-parity", element: <LegacyGuildRedirect target="moderation-desk" /> },
      {
        path: "moderation-desk",
        lazy: async () => ({
          Component: (await import("./features/moderation/pages/ModerationDesk")).ModerationDesk,
        }),
      },
      {
        path: "safety",
        lazy: async () => ({ Component: (await import("./features/moderation/pages/Safety")).Safety }),
      },
      {
        path: "anti-nuke",
        lazy: async () => ({ Component: (await import("./features/moderation/pages/AntiNuke")).AntiNuke }),
      },
      {
        path: "command-control",
        lazy: async () => ({
          Component: (await import("./features/admin/pages/CommandControl")).CommandControl,
        }),
      },
      {
        path: "backup",
        lazy: async () => ({ Component: (await import("./features/admin/pages/ConfigBackup")).ConfigBackup }),
      },
      {
        path: "workflows",
        lazy: async () => ({ Component: (await import("./features/admin/pages/Workflows")).Workflows }),
      },
      {
        path: "automod",
        lazy: async () => ({ Component: (await import("./features/moderation/pages/AutoMod")).AutoMod }),
      },
      {
        path: "cases",
        lazy: async () => ({ Component: (await import("./features/moderation/pages/Cases")).Cases }),
      },
      {
        path: "staff-log",
        lazy: async () => ({ Component: (await import("./features/moderation/pages/StaffLog")).StaffLog }),
      },
      {
        path: "temp-voice",
        lazy: async () => ({ Component: (await import("./features/admin/pages/TempVoice")).TempVoice }),
      },
      {
        path: "birthdays",
        lazy: async () => ({ Component: (await import("./features/community/pages/Birthdays")).Birthdays }),
      },
      {
        path: "commands",
        lazy: async () => ({ Component: (await import("./features/admin/pages/Commands")).Commands }),
      },
      {
        path: "commands/new",
        lazy: async () => ({
          Component: (await import("./features/admin/pages/CommandEditor")).CommandEditor,
        }),
      },
      {
        path: "commands/:commandId",
        lazy: async () => ({
          Component: (await import("./features/admin/pages/CommandEditor")).CommandEditor,
        }),
      },
      {
        path: "responders",
        lazy: async () => ({ Component: (await import("./features/admin/pages/Responders")).Responders }),
      },
      {
        path: "responders/new",
        lazy: async () => ({
          Component: (await import("./features/admin/pages/ResponderEditor")).ResponderEditor,
        }),
      },
      {
        path: "responders/:responderId",
        lazy: async () => ({
          Component: (await import("./features/admin/pages/ResponderEditor")).ResponderEditor,
        }),
      },
      {
        path: "roles",
        lazy: async () => ({ Component: (await import("./features/roles/pages/Roles")).Roles }),
      },
      {
        path: "audit",
        lazy: async () => ({ Component: (await import("./features/moderation/pages/AuditLog")).AuditLog }),
      },
      {
        path: "starboard",
        lazy: async () => ({ Component: (await import("./features/community/pages/Starboard")).Starboard }),
      },
      {
        path: "invites",
        lazy: async () => ({ Component: (await import("./features/community/pages/Invites")).Invites }),
      },
      {
        path: "stats",
        lazy: async () => ({ Component: (await import("./features/admin/pages/Stats")).Stats }),
      },
      {
        path: "utility",
        lazy: async () => ({ Component: (await import("./features/admin/pages/Utility")).Utility }),
      },
      {
        path: "achievements",
        lazy: async () => ({
          Component: (await import("./features/progression/pages/Achievements")).Achievements,
        }),
      },
      {
        path: "tickets/:ticketId?",
        lazy: async () => ({ Component: (await import("./features/community/pages/Tickets")).Tickets }),
      },
      {
        path: "feeds/*",
        lazy: async () => ({ Component: (await import("./features/integrations/pages/Feeds")).Feeds }),
      },
      {
        path: "trackers",
        lazy: async () => ({ Component: (await import("./features/community/pages/Trackers")).Trackers }),
      },
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
  { Component: LocalizedPages, hydrateFallbackElement: <Loading full />, children: localizedRoutes },
  {
    path: "/me/*",
    hydrateFallbackElement: <Loading full />,
    lazy: async () => ({ Component: (await import("./features/games/pages/player/Player")).PlayerRoutes }),
  },
]);
