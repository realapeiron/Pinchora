import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, expect, it } from "vitest";
import { setLocale, t } from "../../i18n";
import { CollectionBook, CollectionWishlist } from "./Collection";

afterEach(() => setLocale("en"));
it.each(["en", "tr"] as const)(
  "collection preserves original and Glimmers and has one title in %s",
  async (lang) => {
    await setLocale(lang);
    const client = new QueryClient();
    client.setQueryData(["player", "collection/book", "en"], {
      items: [
        {
          id: "crab",
          name: "Crab",
          kind: "critter",
          rarity: "Rare",
          caught: 3,
          kept: 3,
          glimmer: 2,
          habitat: "shore",
        },
      ],
      found: 1,
      total: 84,
      shards: 0,
      fragments: 0,
      craft_cost: 12,
      sets: [],
      roll: null,
    });
    client.setQueryData(["player", "collection/wishlist", "en"], { items: [], max: 10 });
    const book = renderToStaticMarkup(
      <QueryClientProvider client={client}>
        <CollectionBook />
      </QueryClientProvider>,
    );
    expect(book.match(/<h1\b/g)).toHaveLength(1);
    expect(book).not.toContain(t("Recycle spare"));
    expect(book).toContain(t("Glimmer variant"));
    const wishes = renderToStaticMarkup(
      <QueryClientProvider client={client}>
        <CollectionWishlist />
      </QueryClientProvider>,
    );
    expect(wishes.match(/<h1\b/g)).toHaveLength(1);
    expect(wishes).not.toMatch(/type="checkbox"[^>]*checked/);
    client.clear();
  },
);
