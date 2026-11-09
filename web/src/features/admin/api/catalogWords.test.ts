import { describe, expect, it } from "vitest";
import { catalogWords } from "./catalogWords";

describe("server UI catalogs", () => {
  it("formats Turkish counts without changing IDs or user text", () => {
    const t = catalogWords(
      {
        "demo.going": "{count} deneme kapasitesi",
        "demo.cancel": "Deneme: {title}",
      },
      "tr",
    );
    expect(t("demo.going", { count: 2500 })).toBe("2.500 deneme kapasitesi");
    expect(t("demo.cancel", { title: "Ada 2500" })).toBe("Deneme: Ada 2500");
    expect(t("#{id}", { id: "123456789" })).toBe("#123456789");
    expect(catalogWords({}, "en")("{count}", { count: 2500 })).toBe("2,500");
  });
});
