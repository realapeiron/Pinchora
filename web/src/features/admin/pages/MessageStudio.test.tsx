import { renderToStaticMarkup } from "react-dom/server";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Outlet, Route, Routes } from "react-router";
import { afterEach, expect, it } from "vitest";
import { setLocale, translate } from "../../../i18n";
import { MessageStudio } from "./MessageStudio";

afterEach(() => setLocale("en"));
it.each(["en", "tr"] as const)(
  "has one title, labeled keyboard inputs and an unchanged disabled save in %s",
  async (lang) => {
    await setLocale(lang);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity } } });
    const html = renderToStaticMarkup(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <Routes>
            <Route element={<Outlet context={{ guild: { id: "1", name: "Reef" } }} />}>
              <Route index element={<MessageStudio />} />
            </Route>
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>,
    );
    expect(html.match(/<h1\b/g)).toHaveLength(1);
    expect(html).toContain(translate("MESSAGE STUDIO"));
    expect(html).toMatch(new RegExp(`<button[^>]*disabled[^>]*>${translate("Save kit")}</button>`));
    expect(html).toContain('for="kit-content"');
    expect(html).toContain('aria-label="' + translate("Saved kit") + '"');
    client.clear();
  },
);
