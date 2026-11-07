import { describe, expect, it } from "vitest";
import { ChatState } from "./chat";
const line = {
  message_id: "8",
  sender_id: "2",
  name: "Deniz",
  text: "Hello",
  created_at: "2026-11-07T10:00:00Z",
  request_id: "3",
};
describe("server acknowledged chat", () => {
  it("stays pending until an acknowledgment and deduplicates the echo", () => {
    const state = new ChatState();
    state.pending("3", "2", "Hello");
    expect(state.snapshot()[0]?.delivery).toBe("pending");
    state.accept(line);
    state.accept(line);
    expect(state.snapshot()).toHaveLength(1);
    expect(state.snapshot()[0]?.delivery).toBe("delivered");
  });
  it("rejections never become delivered", () => {
    const state = new ChatState();
    state.pending("3", "2", "Hello");
    state.reject("3", "chat_muted");
    expect(state.snapshot()[0]).toMatchObject({ delivery: "failed", reason: "chat_muted" });
  });
  it("disconnect leaves uncertain sends failed, never retries automatically", () => {
    const state = new ChatState();
    state.pending("3", "2", "Hello");
    state.disconnected();
    expect(state.snapshot()[0]).toMatchObject({ delivery: "failed", reason: "delivery_unknown" });
  });
  it("policy changes purge transcript and subscriptions update", () => {
    const state = new ChatState();
    let updates = 0;
    const stop = state.subscribe(() => updates++);
    state.accept(line);
    state.clear();
    expect(state.snapshot()).toEqual([]);
    expect(updates).toBe(2);
    stop();
  });
  it("bounds the local transcript and preserves plain text", () => {
    const state = new ChatState();
    for (let i = 0; i < 120; i++)
      state.accept({ ...line, message_id: String(i), request_id: "", text: "<b>hello</b>" });
    expect(state.snapshot()).toHaveLength(100);
    expect(state.snapshot()[0]?.text).toBe("<b>hello</b>");
  });
});
