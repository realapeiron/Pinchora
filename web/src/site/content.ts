// Words and links of the landing page that are not about one feature.

import facts from "../lib/facts.json";

/** The link that adds the bot: the server sends people to Discord's page with the right permissions. */
export const INVITE_URL = "/invite";
export const GITHUB_URL = "https://github.com/realapeiron/Pinchora";
export const DOCS_URL = `${GITHUB_URL}#readme`;
/** The support server. Replace the placeholder with the real invite at build time: `VITE_SUPPORT_URL=... npm run build`. */
export const SUPPORT_URL: string =
  import.meta.env.VITE_SUPPORT_URL ?? "https://discord.gg/REPLACE-WITH-SUPPORT-INVITE";

export const STEPS = [
  {
    title: "Add Pinchora",
    text: "Press Add to Discord, pick your server and press Authorize. Pinchora asks only for what it uses, and the list is in the questions below.",
    detail: "Takes ten seconds",
  },
  {
    title: "Say hello",
    text: "Type /help for the guide, /daily for your first shells and /house view to walk into your new home. Everything is a slash command.",
    detail: "Nothing to install",
  },
  {
    title: "Make it yours",
    text: "Open the dashboard to switch on leveling, welcome cards and level roles, or turn off the parts you do not want.",
    detail: "From any browser",
  },
];

/** Numbers that come from the bot's own content, so the page never promises more than there is. */
export const FACTS = [
  { value: facts.commands, label: "commands" },
  { value: facts.furniture, label: "furniture designs" },
  { value: facts.pets, label: "pet species" },
  { value: facts.fish, label: "fish to catch" },
  { value: facts.backgrounds, label: "profile backgrounds" },
  { value: facts.badges, label: "badges" },
];

export const FAQ: { question: string; answer: string }[] = [
  {
    question: "What permissions does Pinchora ask for?",
    answer:
      "View channels, send messages, embed links, attach files, add reactions, read message history, use external emojis and manage roles. Manage Roles is used only for roles you set up yourself: level roles, reputation roles, the join role, role buttons and the role store. Discord never lets a bot give a role above its own, so move Pinchora's role above the ones it hands out.",
  },
  {
    question: "Who can change a server's settings?",
    answer:
      "Anyone with the Manage Server permission, from the slash commands or from the dashboard. You log in with Discord, and Pinchora reads your name and your server list and nothing else.",
  },
  {
    question: "What does Pinchora store about me?",
    answer:
      "Your Discord ID and what you do with the bot: levels, shells, your house, pets and cards. The /privacy command lets you make your profile private, leave the global boards or hide your house, and Delete my data removes everything about you.",
  },
  {
    question: "Can I turn parts of it off?",
    answer:
      "Yes. Switch whole parts of Pinchora on or off for the server, or only in the channels you pick, with /setup server or in the dashboard.",
  },
  {
    question: "Is there gambling?",
    answer:
      "One optional game, Shell Game, is a gambling-style bet with shells. It stays off until a manager runs /progression economy gambling and has daily limits. Nobody can lose shells in the heist, the party games or the puzzles.",
  },
  {
    question: "Which languages does it speak?",
    answer: "English and Turkish. Names in other scripts and emoji draw correctly on the cards, too.",
  },
  {
    question: "How do I get help or report a bug?",
    answer:
      "Join the support server from the footer, or open an issue on GitHub. Include the command you ran and what you expected.",
  },
];
