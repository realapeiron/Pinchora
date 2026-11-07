// What Pinchora does, by area: the feature grid on the landing page and one page each under /features.
// Data only (no React, no images), so the build can read it to write each page's tags and the sitemap.

export type IconName = "house" | "dice" | "card" | "trophy" | "sliders" | "window";

/** A picture the bot drew for the site (web/src/site/showcase, written by `write_landing_assets`). */
export type ShotKey =
  | "profile"
  | "room"
  | "chess"
  | "connect4"
  | "wordle"
  | "welcome"
  | "rank"
  | "wallet"
  | "pet"
  | "leaderboard"
  | "badges"
  | "levelup";

export interface Feature {
  slug: string;
  title: string;
  icon: IconName;
  /** One line for the grid card. */
  blurb: string;
  /** The page's title and description for search results and link previews. */
  metaTitle: string;
  metaDescription: string;
  /** The page's opening paragraph. */
  lead: string;
  points: { title: string; text: string }[];
  /** Commands worth trying first, as people type them. */
  try: string[];
  shots: { key: ShotKey; alt: string; caption: string }[];
}

export const FEATURES: Feature[] = [
  {
    slug: "house-and-pets",
    title: "House & pets",
    icon: "house",
    blurb: "An isometric pixel room to furnish, a pet to raise and a shore to fish from.",
    metaTitle: "House, pets and fishing | Pinchora",
    metaDescription:
      "Decorate an isometric pixel house inside Discord, adopt pets, send them on trips and fill your fish book. Every member gets a home.",
    lead: "Every member gets a house. Decorate it from inside Discord, invite friends over, raise a pet and go fishing between chats.",
    points: [
      {
        title: "Decorate in Discord",
        text: "Pick furniture, move the cursor on the floor or the walls, then place, turn, flip and pick up. Undo reverses the last ten changes, even after you close the editor.",
      },
      {
        title: "Six tiers of home",
        text: "Start in a Tide Pool Shack and work up to a Sunken Palace with more rooms and more floor. Furniture stays where it is when you move up.",
      },
      {
        title: "Visit, like and sign the guestbook",
        text: "Walk into a friend's house with your pet and your character. Leave a note, give a like, and compete in the weekly room contest.",
      },
      {
        title: "Pets that need you",
        text: "Feed, play, bathe and walk your pet. Send it on field trips that bring souvenirs home, and watch it level up on its own card.",
      },
      {
        title: "Fishing with a fish book",
        text: "Cast, wait for the bite and press Reel in time. Five spots, five rods, baits for rare fish, and a book that rewards every species you complete.",
      },
      {
        title: "A plaza for the whole server",
        text: "Members donate furniture to a shared plaza and managers arrange it, so the server has a place of its own.",
      },
    ],
    try: ["/house view", "/pet adopt", "/fish cast", "/shop market"],
    shots: [
      {
        key: "room",
        alt: "An isometric pixel-art living room with an upright piano, a bed, an arcade cabinet, a rug and a plant, drawn by the bot.",
        caption: "A room as /house view draws it.",
      },
      {
        key: "pet",
        alt: "A pixel pet card showing a pet's portrait, level bar and four need meters.",
        caption: "Your pet's card, with care buttons under it.",
      },
    ],
  },
  {
    slug: "games",
    title: "Games",
    icon: "dice",
    blurb: "Chess, connect four, wordle, trivia, derbies and party games for the whole channel.",
    metaTitle: "Discord games: chess, wordle, party games | Pinchora",
    metaDescription:
      "Duel friends at chess, checkers, connect four and sea fleet, solve the daily wordle and picross, race hermit crabs and play party games in your Discord server.",
    lead: "Quick duels, daily puzzles and noisy party games, all drawn as pixel-art boards in the channel where you play them.",
    points: [
      {
        title: "Board duels with a wager",
        text: "Reef Chess, Pearl Checkers, Connect Four, Sea Fleet and Share or Grab. Optional bets, private moves where they matter, and a rules engine that has been checked against standard perft counts.",
      },
      {
        title: "Daily puzzles",
        text: "One wordle and one picross the same for everyone each day, with streaks, spoiler-free sharing and a practice mode when you want more.",
      },
      {
        title: "Games for the channel",
        text: "Hermit Crab Derby as an animated race, Treasure Heist with crew roles, trivia with hundreds of questions and Storm Surge, where everyone stacks sandbags together.",
      },
      {
        title: "Party games",
        text: "Hot Pearl, Acronym Tide, Caption the Postcard, Wordle Race, Would You Rather and Truth or Dare, for two to ten players.",
      },
      {
        title: "Fair by design",
        text: "Nobody can lose shells in a heist, gambling-style games stay off until a manager turns them on, and every game has daily limits.",
      },
      {
        title: "A book of critters",
        text: "Beachcomb, set crab pots and catch critter drops to fill the Tide Pool Dex of critters and finds.",
      },
    ],
    try: ["/duel chess", "/play wordle", "/play derby", "/party hotpearl"],
    shots: [
      {
        key: "chess",
        alt: "A pixel-art chess board where the pieces are sea creatures: pearls, seahorses, conches, lighthouses, an octopus queen and a crab king.",
        caption: "Reef Chess, played with menus under the board.",
      },
      {
        key: "wordle",
        alt: "A pixel-art wordle card with coloured letter tiles and a keyboard summary.",
        caption: "The daily wordle, private to you.",
      },
      {
        key: "connect4",
        alt: "A pixel-art connect four board with coral and gold discs.",
        caption: "Connect Four with column buttons.",
      },
    ],
  },
  {
    slug: "profile-cards",
    title: "Profile cards",
    icon: "card",
    blurb: "Pixel-art profile, rank and wallet cards with backgrounds, badges and a character you design.",
    metaTitle: "Pixel profile, rank and wallet cards | Pinchora",
    metaDescription:
      "Collectible Discord profile cards: 36 backgrounds, 41 badges, titles, frames and a pixel character you design. Rank and wallet cards match.",
    lead: "Your profile is a card you can be proud of. Pick a background, wear your best badges and show off a pet, a fish and a house.",
    points: [
      {
        title: "Design your character",
        text: "A private editor with a live preview: looks, skin, hair, eyes, clothes, accessories, presets and saved outfits, with twenty steps of undo.",
      },
      {
        title: "Backgrounds, badges and titles",
        text: "36 backgrounds in five rarities, 41 badges earned through play and celebrations or bought with shells, and titles that follow your stats.",
      },
      {
        title: "A showcase row",
        text: "Put a favourite fish, a pet, a piece of furniture and a thumbnail of your house on the card.",
      },
      {
        title: "Your colours",
        text: "Choose an accent colour, how solid the panels are and a frame around the card, from plain rope up to neon.",
      },
      {
        title: "Rank and wallet to match",
        text: "The rank card shows your level and the next role to earn. The wallet card shows shells and when each reward is ready.",
      },
      {
        title: "Privacy you control",
        text: "Make your profile private, leave the global boards or hide your house with a single command.",
      },
    ],
    try: ["/profile", "/customize background", "/character", "/customize badges"],
    shots: [
      {
        key: "profile",
        alt: "A pixel-art Discord profile card with an avatar, a character, bio text, level, shells, ranks, badges and a showcase row.",
        caption: "A profile card with badges and a showcase.",
      },
      {
        key: "badges",
        alt: "A grid of pixel-art badges, earned ones in colour on rarity plates and locked ones dark with progress bars.",
        caption: "The badge collection.",
      },
      {
        key: "wallet",
        alt: "A pixel-art wallet card with shells, pearls, a streak counter and the time left on each reward.",
        caption: "The wallet card.",
      },
    ],
  },
  {
    slug: "leveling-and-economy",
    title: "Leveling & economy",
    icon: "trophy",
    blurb: "XP for chat and voice, level roles, shells, shops, player stalls and leaderboards.",
    metaTitle: "Leveling, level roles and a shell economy | Pinchora",
    metaDescription:
      "Chat and voice XP with level roles, XP multipliers, weekly boards, daily rewards, a shell economy, a furniture shop and player stalls for your Discord server.",
    lead: "Give your members a reason to come back: levels that earn roles, rewards on a rhythm and shells to spend on things they can see.",
    points: [
      {
        title: "Levels and roles",
        text: "Chatting earns XP and server points. Level roles are given on level-up, and you decide whether members keep every role or only the highest.",
      },
      {
        title: "Voice XP and multipliers",
        text: "Switch on voice XP, then scale what any channel, category or role earns from nothing up to five times.",
      },
      {
        title: "Leaderboards that reset",
        text: "Server and global XP, weekly XP with a podium, weekly biggest catch and the richest shell collectors.",
      },
      {
        title: "Daily, weekly and monthly",
        text: "Rewards with streaks that grow for twenty days, reminders by direct message if you want them, and three daily quests.",
      },
      {
        title: "Shops and stalls",
        text: "An hourly market, a catalogue of 650 furniture designs, gifts, and player stalls where members sell to each other.",
      },
      {
        title: "A store for your server",
        text: "Put roles on sale for server points, with optional stock. Rename the points and pick their emoji.",
      },
    ],
    try: ["/rank", "/daily", "/leaderboard", "/shop catalog"],
    shots: [
      {
        key: "rank",
        alt: "A pixel-art rank card with an avatar, level, an XP bar and the next level role to earn.",
        caption: "The rank card.",
      },
      {
        key: "leaderboard",
        alt: "A pixel-art leaderboard card listing ten members with avatars, levels and XP.",
        caption: "The server leaderboard.",
      },
      {
        key: "levelup",
        alt: "A pixel-art level-up card announcing a jump from level 26 to level 27.",
        caption: "A level-up announcement.",
      },
    ],
  },
  {
    slug: "server-tools",
    title: "Server tools",
    icon: "sliders",
    blurb: "Welcome cards, polls, suggestions, giveaways, role buttons, logs and join roles.",
    metaTitle: "Welcome cards, polls, giveaways and logs | Pinchora",
    metaDescription:
      "Welcome cards, polls, suggestions, giveaways, role button panels, goodbye messages, join roles and a server log for your Discord server.",
    lead: "The everyday jobs of running a server, done with the same care as the games: clear cards, buttons that explain themselves and settings you can change in a browser.",
    points: [
      {
        title: "Welcome cards",
        text: "Greet new members with a pixel card in your own colours and words, and say goodbye when they leave.",
      },
      {
        title: "Polls and suggestions",
        text: "Polls with hidden votes and a results card, and a suggestion box with live counts that managers approve or deny.",
      },
      {
        title: "Giveaways",
        text: "Enter with a button, require a role, give shells as prizes, end early and reroll for new winners.",
      },
      {
        title: "Role buttons",
        text: "Post panels of role buttons that update in place when you change them.",
      },
      {
        title: "Join roles and a server log",
        text: "Give every new member a role, and log joins, leaves, name changes, bans and voice activity to a channel you choose.",
      },
      {
        title: "Modules you can switch off",
        text: "Turn whole parts of Pinchora on or off for the server, or only in the channels you pick.",
      },
    ],
    try: ["/config welcome", "/poll", "/giveaway start", "/config modules"],
    shots: [
      {
        key: "welcome",
        alt: "A pixel-art welcome card with an avatar, the word Welcome and the member number of a server called The Tide Pool.",
        caption: "A welcome card.",
      },
      {
        key: "levelup",
        alt: "A pixel-art level-up card announcing a jump from level 26 to level 27.",
        caption: "Level-up messages, in the channel or by direct message.",
      },
    ],
  },
  {
    slug: "web-dashboard",
    title: "Web dashboard",
    icon: "window",
    blurb: "Change leveling, welcome cards, points, logs and modules from your browser, with a live preview.",
    metaTitle: "The Pinchora web dashboard | Pinchora",
    metaDescription:
      "Log in with Discord and manage leveling, welcome cards, points, logs and modules for your server from a browser, with live card previews and an audit log.",
    lead: "Everything a manager sets up with slash commands, on one page with previews. Log in with Discord, pick a server and change it.",
    points: [
      {
        title: "Live previews",
        text: "Edit a welcome message or a level-up line and see the real card the bot will send, drawn by the same code.",
      },
      {
        title: "Instant checks",
        text: "Limits and placeholders are checked as you type with the exact words the bot would answer with.",
      },
      {
        title: "Changes apply at once",
        text: "A saved setting reaches every part of the bot straight away. No restart, no waiting for a cache to expire.",
      },
      {
        title: "An audit log",
        text: "See who changed what, from the dashboard or from Discord, with the values before and after.",
      },
      {
        title: "Only for managers",
        text: "You need Manage Server in a server to change it. Pinchora asks Discord for your name and server list and nothing else.",
      },
      {
        title: "Your house, too",
        text: "Open your house in the browser and look around your own rooms drawn in the same isometric art.",
      },
    ],
    try: ["Open the dashboard"],
    shots: [
      {
        key: "welcome",
        alt: "A pixel-art welcome card, the kind the dashboard previews as you edit.",
        caption: "The welcome card, previewed live while you edit.",
      },
    ],
  },
];

export function featureBySlug(slug: string | undefined): Feature | undefined {
  return FEATURES.find((f) => f.slug === slug);
}
