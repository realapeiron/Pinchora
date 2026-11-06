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
    timestamp: new Date().toISOString(),
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

function send(res: ServerResponse, status: number, body: unknown) {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
}

createServer((req, res) => {
  req.resume();
  const url = new URL(req.url ?? "/", "http://mock");
  const parts = url.pathname.split("/").filter(Boolean);
  if (url.pathname === "/health") return send(res, 200, { ok: true });
  const [api, , resource, id, sub] = parts;
  if (api === "api" && resource === "guilds" && id && !sub && req.method === "GET") {
    return botIn(id)
      ? send(res, 200, guildJson(id))
      : send(res, 404, { message: "Unknown Guild", code: 10004 });
  }
  if (api === "api" && resource === "channels" && id && sub === "messages" && req.method === "POST") {
    return send(res, 200, messageJson(id));
  }
  send(res, 404, { message: "404: Not Found", code: 0 });
}).listen(port, "127.0.0.1", () => console.log(`mock discord on 127.0.0.1:${port}`));
