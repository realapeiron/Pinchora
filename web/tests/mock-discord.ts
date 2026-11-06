// A tiny stand-in for Discord's REST API for the Playwright smoke run. The Rust server reaches it
// twice: the bot's own calls through twilight's proxy setting (`/api/v10/...`), and the OAuth/user
// calls through PINCHORA_WEB__DISCORD_API_BASE. Dev login skips OAuth, so only bot routes matter.
//
//   MOCK_PORT=18191 node --experimental-strip-types tests/mock-discord.ts

import { createServer, type ServerResponse } from "node:http";

const port = Number(process.env.MOCK_PORT ?? 18191);

/** Server ids ending in 9 do not have the bot, so the picker offers to add it there. */
function botIn(guild: string): boolean {
  return !guild.endsWith("9");
}

function guildJson(id: string) {
  return {
    id,
    name: "Mock Server",
    icon: null,
    splash: null,
    discovery_splash: null,
    owner_id: "1",
    afk_channel_id: null,
    afk_timeout: 300,
    application_id: null,
    banner: null,
    default_message_notifications: 0,
    description: null,
    emojis: [],
    explicit_content_filter: 0,
    features: [],
    large: false,
    mfa_level: 0,
    nsfw_level: 0,
    preferred_locale: "en-US",
    premium_progress_bar_enabled: false,
    public_updates_channel_id: null,
    roles: [],
    rules_channel_id: null,
    system_channel_flags: 0,
    system_channel_id: null,
    vanity_url_code: null,
    verification_level: 0,
  };
}

function messageJson(channel: string) {
  return {
    id: String(Date.now()),
    channel_id: channel,
    type: 0,
    author: { id: "1", username: "pinchora", discriminator: "0", global_name: null, avatar: null },
    content: "",
    timestamp: "2024-01-01T00:00:00.000000+00:00",
    edited_timestamp: null,
    tts: false,
    mention_everyone: false,
    mentions: [],
    mention_roles: [],
    pinned: false,
    attachments: [],
    embeds: [],
    flags: 0,
  };
}

const BOT_ROLE = "9000";

function role(id: string, name: string, position: number, color: number, managed = false, permissions = "0") {
  return {
    id,
    name,
    color,
    colors: { primary_color: color, secondary_color: null, tertiary_color: null },
    hoist: false,
    managed,
    mentionable: false,
    permissions,
    position,
    flags: 0,
  };
}

/** The roles of every mock server: some below the bot's, one above it, one an integration owns. */
function rolesJson(guild: string) {
  return [
    role(guild, "@everyone", 0, 0, false, "2048"),
    role(BOT_ROLE, "Pinchora", 10, 0xff6b4a, true, String(1 << 28)),
    role("9001", "Fan", 3, 0x06d6a0),
    role("9002", "Regular", 5, 0x8fd3ff),
    role("9005", "Crab Collector", 4, 0xffd166),
    role("9006", "Veteran", 6, 0xff8fb1),
    role("9003", "Admin", 20, 0xff4d6d, false, "8"),
    role("9004", "Server Booster", 2, 0xf47fff, true),
  ];
}

function channel(
  guild: string,
  id: string,
  name: string,
  type: number,
  position: number,
  parent: string | null,
) {
  return {
    id,
    guild_id: guild,
    type,
    name,
    position,
    parent_id: parent,
    permission_overwrites: [],
    nsfw: false,
  };
}

function channelsJson(guild: string) {
  return [
    channel(guild, "7000", "Info", 4, 0, null),
    channel(guild, "7010", "welcome", 0, 1, "7000"),
    channel(guild, "7011", "announcements", 5, 2, "7000"),
    channel(guild, "7004", "Chat", 4, 3, null),
    channel(guild, "7001", "general", 0, 4, "7004"),
    channel(guild, "7002", "memes", 0, 5, "7004"),
    channel(guild, "7003", "Lounge", 2, 6, "7004"),
    channel(guild, "7020", "Staff", 4, 7, null),
    channel(guild, "7021", "mod-log", 0, 8, "7020"),
  ];
}

function memberJson(user: string, roles: string[]) {
  return {
    user: { id: user, username: `user${user}`, discriminator: "0", global_name: null, avatar: null },
    roles,
    joined_at: "2024-01-01T00:00:00.000000+00:00",
    deaf: false,
    mute: false,
    flags: 0,
    pending: false,
  };
}

/** Names for the members the ticket seed (`seed_demo_tickets`) uses; anyone else is `member` and a number. */
const NAMES: Record<string, string> = {
  "770000000000000101": "Sam",
  "770000000000000102": "Mia",
  "770000000000000103": "Kai",
  "770000000000000104": "Leo",
};

function userJson(id: string) {
  return {
    id,
    username: `member${id.slice(-3)}`,
    discriminator: "0",
    global_name: NAMES[id] ?? null,
    avatar: null,
  };
}

/** Roles members were given, by user id. */
const held = new Map<string, Set<string>>();

function send(res: ServerResponse, status: number, body: unknown) {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
}

createServer((req, res) => {
  req.resume();
  const url = new URL(req.url ?? "/", "http://mock");
  const parts = url.pathname.split("/").filter(Boolean);
  if (url.pathname === "/health") return send(res, 200, { ok: true });
  const [api, , resource, id, sub, member, extra, roleId] = parts;
  if (api === "api" && resource === "guilds" && id && sub === "channels" && req.method === "GET") {
    return send(res, 200, channelsJson(id));
  }
  if (api === "api" && resource === "guilds" && id && sub === "roles" && req.method === "GET") {
    return send(res, 200, rolesJson(id));
  }
  if (
    api === "api" &&
    resource === "guilds" &&
    sub === "members" &&
    member &&
    !extra &&
    req.method === "GET"
  ) {
    const roles = member === "1" ? [BOT_ROLE] : [...(held.get(member) ?? [])];
    return send(res, 200, memberJson(member, roles));
  }
  if (api === "api" && resource === "guilds" && sub === "members" && member && extra === "roles" && roleId) {
    const roles = held.get(member) ?? new Set<string>();
    if (req.method === "PUT") roles.add(roleId);
    else roles.delete(roleId);
    held.set(member, roles);
    res.writeHead(204).end();
    return;
  }
  if (api === "api" && resource === "guilds" && id && !sub && req.method === "GET") {
    return botIn(id)
      ? send(res, 200, guildJson(id))
      : send(res, 404, { message: "Unknown Guild", code: 10004 });
  }
  if (api === "api" && resource === "channels" && id && sub === "messages" && req.method === "POST") {
    return send(res, 200, messageJson(id));
  }
  if (api === "api" && resource === "users" && id && id !== "@me" && !sub && req.method === "GET") {
    return send(res, 200, userJson(id));
  }
  send(res, 404, { message: "404: Not Found", code: 0 });
}).listen(port, "127.0.0.1", () => console.log(`mock discord on 127.0.0.1:${port}`));
