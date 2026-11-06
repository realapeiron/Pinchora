import { describe, expect, it } from "vitest";

import { jobProgress, jobText } from "./syncJob";

describe("sync job progress", () => {
  it("is empty until a count comes in, and never divides by zero", () => {
    expect(jobProgress(undefined)).toBe(0);
    expect(jobProgress({ status: "running", checked: 0, total: 0 })).toBe(0);
    expect(jobText({ checked: 0, total: 0 })).toBe("Looking for members with XP…");
    expect(jobText(undefined)).toBe("Looking for members with XP…");
  });

  it("follows the count while running", () => {
    expect(jobProgress({ status: "running", checked: 25, total: 100 })).toBe(0.25);
    expect(jobText({ checked: 25, total: 100 })).toBe("Checked 25 of 100 members");
    expect(jobProgress({ status: "running", checked: 120, total: 100 })).toBe(1);
  });

  it("is full when finished, even with nobody to check", () => {
    expect(jobProgress({ status: "done", checked: 0, total: 0 })).toBe(1);
  });
});
