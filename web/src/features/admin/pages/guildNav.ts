import type { NavGroup } from "../../../lib/nav";

/**
 * The admin pages of a server, by sidebar group. Paths are relative to `/g/:guildId`. A feature adds its
 * page as one line in its own group; a group with no page yet is not shown.
 */
export const NAV: NavGroup[] = [
  {
    group: "SERVER",
    pages: [
      { to: "", label: "Overview" },
      { to: "modules", label: "Modules" },
      { to: "branding", label: "Branding" },
      { to: "command-control", label: "Commands" },
      { to: "backup", label: "Backup" },
      { to: "audit", label: "Audit log" },
      { to: "access", label: "Access" },
      { to: "stats", label: "Stats" },
      { to: "invites", label: "Invites" },
      { to: "utility", label: "Messages" },
      { to: "message-studio", label: "Message studio" },
    ],
  },
  {
    group: "MEMBERS",
    pages: [
      { to: "welcome", label: "Welcome & goodbye" },
      { to: "welcome-image", label: "Welcome image" },
      { to: "leveling", label: "Leveling" },
      { to: "points", label: "Points & store" },
      { to: "economy", label: "Economy settings" },
    ],
  },
  {
    group: "TOOLS",
    pages: [
      { to: "commands", label: "Custom commands" },
      { to: "workflows", label: "Workflows" },
      { to: "responders", label: "Auto-responders" },
      { to: "community", label: "Community" },
    ],
  },
  { group: "ROLES", pages: [{ to: "roles", label: "Roles" }] },
  {
    group: "SAFETY",
    pages: [
      { to: "moderation-desk", label: "Moderation desk" },
      { to: "moderation", label: "Moderation" },
      { to: "safety", label: "Safety" },
      { to: "automod", label: "AutoMod" },
      { to: "anti-nuke", label: "Anti-nuke" },
      { to: "cases", label: "Cases" },
      { to: "log", label: "Server log" },
      { to: "staff-log", label: "Staff log" },
    ],
  },
  {
    group: "COMMUNITY",
    pages: [
      { to: "tickets", label: "Tickets" },
      { to: "events", label: "Events" },
      { to: "temp-voice", label: "Temp voice" },
      { to: "starboard", label: "Starboard" },
      { to: "birthdays", label: "Birthdays" },
      { to: "community-engagement", label: "Community pack" },
      { to: "giveaways", label: "Giveaways" },
      { to: "polls", label: "Polls" },
      { to: "suggestions", label: "Suggestions" },
      { to: "reminders", label: "Reminders" },
      { to: "highlights", label: "Highlights" },
      { to: "auto-channels", label: "Auto channels" },
      { to: "anniversaries", label: "Anniversaries" },
      { to: "confessions", label: "Confessions" },
    ],
  },
  {
    group: "FEEDS",
    pages: [
      { to: "feeds", label: "Social feeds" },
      { to: "roblox", label: "Roblox" },
      { to: "trackers", label: "Game trackers" },
    ],
  },
  { group: "PLAYERS", pages: [{ to: "achievements", label: "Achievements" }] },
];
