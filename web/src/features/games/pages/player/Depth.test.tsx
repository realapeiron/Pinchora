import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, expect, it } from "vitest";
import { setLocale, t } from "../../../../i18n";
import { Friends, Guestbook, Pets } from "./Depth";

afterEach(() => setLocale("en"));

it.each(["en", "tr"] as const)(
  "shows one friends heading and honest social empty states in %s",
  async (language) => {
    await setLocale(language);
    const client = new QueryClient();
    // Server rendering uses the en query key; displayed copy still follows the selected locale.
    client.setQueryData(["player", "friends?page=1", "en"], { items: [], total: 0 });
    client.setQueryData(["player", "partner", "en"], { partner: null });
    client.setQueryData(["player", "guestbook?page=1", "en"], { items: [], total: 0, blocks: [] });
    const friends = renderToStaticMarkup(
      <QueryClientProvider client={client}>
        <Friends />
      </QueryClientProvider>,
    );
    expect(friends.match(/<h1\b/g)).toHaveLength(1);
    expect(friends).not.toContain(`class="pixel-title">${t("Friends")}</h2>`);
    expect(friends.match(new RegExp(t("Nothing here yet."), "g"))).toHaveLength(2);
    const guestbook = renderToStaticMarkup(
      <QueryClientProvider client={client}>
        <Guestbook />
      </QueryClientProvider>,
    );
    expect(guestbook.match(new RegExp(t("Nothing here yet."), "g"))).toHaveLength(2);
    client.clear();
  },
);

it("does not show an empty partner state while the query is loading", () => {
  const client = new QueryClient();
  const html = renderToStaticMarkup(
    <QueryClientProvider client={client}>
      <Friends />
    </QueryClientProvider>,
  );
  expect(html).not.toContain(t("Nothing here yet."));
  client.clear();
});

it("renders pet names and needs safely with rename disabled until a change", () => {
  const client = new QueryClient();
  client.setQueryData(["player", "pets", "en"], {
    items: [
      {
        id: "42",
        name: "<img onerror=evil>",
        species_name: "Crab",
        level: 1,
        xp: 0,
        active: true,
        status: "awake",
        needs: { hunger: 50, fun: 50, energy: 50, hygiene: 50 },
        trip_dest: null,
      },
    ],
    trips: [],
  });
  const html = renderToStaticMarkup(
    <QueryClientProvider client={client}>
      <Pets />
    </QueryClientProvider>,
  );
  expect(html).toContain("&lt;img onerror=evil&gt;");
  expect(html).toMatch(/<button[^>]*disabled=""[^>]*>Rename<\/button>/);
  expect(html.match(/<progress\b/g)).toHaveLength(4);
  client.clear();
});
