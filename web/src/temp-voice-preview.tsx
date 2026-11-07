import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createRoot } from "react-dom/client";
import { createMemoryRouter, Outlet, RouterProvider } from "react-router";

import type { GuildInfo, Overview } from "./lib/api";
import type { TempVoice as TempVoiceData } from "./lib/tempVoice";
import { TempVoice } from "./pages/TempVoice";
import "./styles/tokens.css";
import "./styles/app.css";
import "./styles/site.css";

const guild = "123456789012345678";
const info: GuildInfo = {
  channels: [
    { id: "7004", name: "Rooms", type: 4, parent_id: null, position: 0 },
    { id: "7003", name: "Lounge", type: 2, parent_id: "7004", position: 1 },
    {
      id: "7007",
      name: "A very long creator channel name that should stay inside the row",
      type: 2,
      parent_id: "7004",
      position: 2,
    },
    { id: "7008", name: "Open mic", type: 2, parent_id: null, position: 3 },
    { id: "7001", name: "general", type: 0, parent_id: null, position: 4 },
    { id: "7005", name: "Mira's room", type: 2, parent_id: "7004", position: 5 },
    {
      id: "7006",
      name: "The longest possible late night study room with a few more words",
      type: 2,
      parent_id: "7004",
      position: 6,
    },
  ],
  roles: [],
  bot: { top_role_position: 2, manage_roles: true },
};
const data: TempVoiceData = {
  enabled: true,
  missing: ["Manage Channels"],
  creators: [
    {
      channel_id: "7003",
      category_id: "7004",
      name_template: "{user}'s room",
      default_limit: 4,
      default_locked: true,
      max_per_user: 2,
      panel: true,
      panel_channel_id: "7001",
      enabled: true,
      rooms: 2,
    },
    {
      channel_id: "7007",
      category_id: "7004",
      name_template: "Room {n} · {user}",
      default_limit: 0,
      default_locked: false,
      max_per_user: 1,
      panel: true,
      panel_channel_id: null,
      enabled: false,
      rooms: 0,
    },
  ],
  rooms: [
    {
      channel_id: "7005",
      source_id: "7003",
      owner_id: "8",
      owner_name: "Mira",
      created_at: new Date(Date.now() - 12 * 60_000).toISOString(),
      locked: true,
      hidden: false,
      user_limit: 4,
      members: 3,
    },
    {
      channel_id: "7006",
      source_id: "7003",
      owner_id: "9",
      owner_name: "A member with a long display name",
      created_at: new Date(Date.now() - 3 * 60 * 60_000).toISOString(),
      locked: false,
      hidden: true,
      user_limit: 0,
      members: 1,
    },
  ],
  limits: {
    max_creators: 10,
    max_rooms: 60,
    max_per_user: 5,
    max_template: 80,
    max_limit: 99,
    default_template: "{user}'s room",
  },
};

const fetchReal = window.fetch.bind(window);
window.fetch = async (input, init) => {
  const path = typeof input === "string" ? input : input instanceof Request ? input.url : String(input);
  const body = path.endsWith("/temp-voice") ? data : path.endsWith("/discord") ? info : null;
  if (body)
    return new Response(JSON.stringify(body), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  return fetchReal(input, init);
};

const overview = {
  guild: { id: guild, name: "Pinchora Reef", icon_url: null },
  settings: {},
  modules: [],
  audit: [],
} as unknown as Overview;
function Layout() {
  return (
    <div className="page">
      <div className="guild">
        <aside className="sidebar">
          <div className="sidebar-server">
            <strong className="sidebar-server-name">Pinchora Reef</strong>
          </div>
          <nav className="sidebar-nav" aria-label="Server settings">
            <div className="sidebar-section">
              <p className="sidebar-group pixel-label">COMMUNITY</p>
              <a className="sidebar-link active" href="#">
                Temp voice
              </a>
            </div>
          </nav>
        </aside>
        <main className="content">
          <Outlet context={overview} />
        </main>
      </div>
    </div>
  );
}
const router = createMemoryRouter([
  { path: "/", element: <Layout />, children: [{ index: true, element: <TempVoice /> }] },
]);
createRoot(document.getElementById("root")!).render(
  <QueryClientProvider client={new QueryClient()}>
    <RouterProvider router={router} />
  </QueryClientProvider>,
);
