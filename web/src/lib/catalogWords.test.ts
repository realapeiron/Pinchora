import { describe, expect, it } from "vitest";
import { catalogWords } from "./catalogWords";

describe("server UI catalogs", () => {
  it("formats Turkish counts without changing IDs or user text", () => {
    const t = catalogWords(
      {
        "{count} going places": "{count} kişilik kapasite",
        "Cancel {title}?": "{title} adlı etkinlik iptal edilsin mi?",
      },
      "tr",
    );
    expect(t("{count} going places", { count: 2500 })).toBe("2.500 kişilik kapasite");
    expect(t("Cancel {title}?", { title: "Ada 2500" })).toBe("Ada 2500 adlı etkinlik iptal edilsin mi?");
    expect(t("#{id}", { id: "123456789" })).toBe("#123456789");
    expect(catalogWords({}, "en")("{count}", { count: 2500 })).toBe("2,500");
  });
});
