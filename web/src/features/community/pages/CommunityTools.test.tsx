import { renderToStaticMarkup } from "react-dom/server";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Outlet, Route, Routes } from "react-router";
import { afterEach, expect, it } from "vitest";
import { setLocale } from "../../../i18n";
import {
  CommunityGiveaways,
  CommunityPolls,
  CommunitySuggestions,
  CommunityReminders,
  CommunityHighlights,
  CommunityAutoChannels,
  CommunityAnniversaries,
  CommunityConfessions,
} from "./CommunityTools";

const data = {
  giveaways: [],
  polls: [],
  suggestions: [],
  autopost: [],
  anniversary: { channel_id: null, enabled: false },
  confession_settings: { review_channel_id: null, publish_channel_id: null, enabled: false },
  confessions: [],
  message_content: false,
};
afterEach(() => setLocale("en"));

it.each(["en", "tr"] as const)(
  "community pages have one title and unchanged forms cannot save in %s",
  async (lang) => {
    await setLocale(lang);
    for (const Page of [
      CommunityGiveaways,
      CommunityPolls,
      CommunitySuggestions,
      CommunityReminders,
      CommunityHighlights,
      CommunityAutoChannels,
      CommunityAnniversaries,
      CommunityConfessions,
    ]) {
      const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity } } });
      client.setQueryData(["community-v2", "1"], data);
      client.setQueryData(["community-v2-personal", "1"], { reminders: [], checklist: [], highlights: [] });
      const html = renderToStaticMarkup(
        <QueryClientProvider client={client}>
          <MemoryRouter initialEntries={["/"]}>
            <Routes>
              <Route element={<Outlet context={{ guild: { id: "1" } }} />}>
                <Route index element={<Page />} />
              </Route>
            </Routes>
          </MemoryRouter>
        </QueryClientProvider>,
      );
      expect(html.match(/<h1\b/g), Page.name).toHaveLength(1);
      for (const button of html.matchAll(/<button\b([^>]*)>(Save|Kaydet)<\/button>/g)) {
        expect(button[1], Page.name).toContain("disabled");
      }
      if (Page === CommunityHighlights)
        expect(html).toContain(lang === "en" ? "Needs Message Content intent" : "Mesaj İçeriği");
      client.clear();
    }
  },
);
