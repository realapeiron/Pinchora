import { it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { TradeReview } from "./Economy";
it("requires review, shows both snowflake IDs and escapes item names", () => {
  const html = renderToStaticMarkup(
    <TradeReview
      busy={false}
      run={() => {}}
      trade={{
        id: 1,
        user_a: "123456789123456789",
        user_b: "987654321987654321",
        offer_a: { shells: 500, items: [{ item_id: "<img onerror=evil>", quantity: 2 }] },
        offer_b: { shells: 0, items: [] },
        confirmed_a: false,
        confirmed_b: false,
        expires_at: "2026-12-01T00:00:00Z",
        status: "open",
      }}
    />,
  );
  expect(html).toContain("123456789123456789");
  expect(html).toContain("987654321987654321");
  expect(html).toContain("&lt;img onerror=evil&gt;");
  expect(html).toMatch(/disabled=""[^>]*>Confirm this exact trade/);
  expect(html).toContain("500");
});
