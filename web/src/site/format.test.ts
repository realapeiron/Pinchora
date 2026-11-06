import { describe, expect, it } from "vitest";

import { compact } from "./format";

describe("compact", () => {
  it("keeps small numbers whole, with separators", () => {
    expect(compact(0)).toBe("0");
    expect(compact(987)).toBe("987");
    expect(compact(9_999)).toBe("9,999");
  });
  it("shortens big ones", () => {
    expect(compact(10_000)).toBe("10K");
    expect(compact(12_345)).toBe("12.3K");
    expect(compact(250_400)).toBe("250K");
    expect(compact(1_250_000)).toBe("1.3M");
  });
});
