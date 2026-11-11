import type { AchievementDraft } from "../api/achievements";

export const SYMBOLS: Record<string, string> = {
  star: "★",
  shell: "◉",
  fish: "◆",
  crab: "✦",
  crown: "♛",
  heart: "♥",
  spark: "✧",
  trophy: "★",
  chat: "▣",
  clock: "◷",
  ticket: "▤",
  gem: "♦",
};
export const KIND_LABELS: Record<string, string> = {
  messages: "Messages",
  voice_minutes: "Voice minutes",
  level: "Level",
  days_active: "Days active",
  invites: "Invites",
  stars_received: "Stars received",
  tickets_closed: "Tickets closed",
  reputation: "Reputation",
  games_won: "Games won",
  fish_caught: "Fish caught",
  custom: "Custom",
};
export const KIND_MODULES: Record<string, string> = {
  messages: "stats",
  voice_minutes: "stats",
  days_active: "stats",
  level: "leveling",
  invites: "invites",
  stars_received: "starboard",
  tickets_closed: "tickets",
  reputation: "economy",
  games_won: "profile",
  fish_caught: "fishing",
};
export const fresh = (): AchievementDraft => ({
  name: "",
  description: "",
  icon: "star",
  kind: "messages",
  target: 10,
  reward_role_id: null,
  reward_points: 0,
  announce_channel_id: null,
  secret: false,
  enabled: true,
});
