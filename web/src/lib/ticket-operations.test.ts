import { describe, expect, it } from "vitest";
import { operationSettingsBody, snippetChanged } from "./ticket-operations";

describe("ticket operation edit boundaries", () => {
  it("enables saving replies only when the trimmed stored value changes", () => {
    const saved = [{ id: 1, name: "Welcome", body: "Hello" }];
    expect(snippetChanged(saved, " Welcome ", " Hello ")).toBe(false);
    expect(snippetChanged(saved, "", "Hello")).toBe(false);
    expect(snippetChanged(saved, "Welcome", " ")).toBe(false);
    expect(snippetChanged(saved, "Welcome", "Updated")).toBe(true);
  });
  it("excludes readonly and unexpected fields from operation payloads", () => {
    const settings = {
      support_timezone: "UTC",
      support_hours: [],
      out_of_hours_message: "",
      status_channel_id: "123",
      status_message_id: "456",
      close_timeout_hours: 24,
      unexpected: "do not send",
    };
    expect(operationSettingsBody(settings)).toEqual({
      support_timezone: "UTC",
      support_hours: [],
      out_of_hours_message: "",
      status_channel_id: "123",
      close_timeout_hours: 24,
    });
  });
});
