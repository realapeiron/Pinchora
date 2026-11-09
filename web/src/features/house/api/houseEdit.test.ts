import { describe, expect, it, vi } from "vitest";
import { RoomQueue, optimistic, sendOperation, type Snapshot } from "./houseEdit";
import { ApiError } from "../../../lib/api";
import type { RoomJson } from "../../../iso/scene";
import { canPlace } from "../../../iso/rules";
import { fixtureContent } from "../../../iso/testing";

const room: RoomJson = {
  idx: 0,
  room_type: "living",
  width: 8,
  depth: 8,
  wallpaper: "plain_cream",
  floor: "oak_planks",
  placements: [],
  pets: [],
  characters: [],
  seed: 0,
  fingerprint: "first",
  undo: [],
};
const place = { type: "place" as const, item_id: "plainwhorl_chair__sand", x: 0, y: 0, flipped: false };
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
describe("house operations queue", () => {
  it("serializes writes and reconciles temporary ids before dependent operations", async () => {
    let complete!: (value: Snapshot) => void;
    const send = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<Snapshot>((resolve) => {
            complete = resolve;
          }),
      )
      .mockResolvedValueOnce({
        fingerprint: "third",
        placements: [{ ...place, id: "123", flipped: true }],
        undo: [],
      });
    const changed = vi.fn();
    const q = new RoomQueue(room, send, async () => room, changed);
    q.enqueue(place);
    q.enqueue({ type: "flip", id: "pending-1" });
    expect(send).toHaveBeenCalledTimes(1);
    complete({ fingerprint: "second", placements: [{ ...place, id: "123" }], undo: [] });
    await tick();
    expect(send).toHaveBeenNthCalledWith(2, "second", { type: "flip", id: "123" });
    expect(q.room.placements[0]?.flipped).toBe(true);
  });
  it("drops dependent edits and reloads the authoritative room on 409", async () => {
    const send = vi.fn().mockRejectedValue(new ApiError(409, "room_changed", "changed"));
    const changed = vi.fn();
    const fresh = { ...room, fingerprint: "elsewhere" };
    const q = new RoomQueue(room, send, async () => fresh, changed);
    q.enqueue(place);
    q.enqueue({ type: "clear" });
    await tick();
    expect(send).toHaveBeenCalledTimes(1);
    expect(q.room).toBe(fresh);
    expect(changed.mock.lastCall?.[2]).toContain("Discord?");
  });
  it("rolls back validation failures and never replays them", async () => {
    const q = new RoomQueue(
      room,
      async () => {
        throw new ApiError(422, "blocked", "something is already in that spot");
      },
      async () => room,
      () => {},
    );
    q.enqueue(place);
    await tick();
    expect(q.room.placements).toEqual([]);
    expect(q.pending).toBe(false);
  });
  it("keeps purchase confirmation separate from room conflicts", async () => {
    const changed = vi.fn();
    const q = new RoomQueue(
      room,
      async () => {
        throw new ApiError(409, "confirm", "Confirm this purchase.");
      },
      async () => room,
      changed,
    );
    q.enqueue({ type: "surface", wallpaper: "new" });
    await tick();
    expect(q.room.wallpaper).toBe(room.wallpaper);
    expect(changed.mock.lastCall?.[2]).toBe("Confirm this purchase.");
  });
  it("does not let passive refresh replace pending work", async () => {
    const q = new RoomQueue(
      room,
      () => new Promise(() => {}),
      async () => room,
      () => {},
    );
    q.enqueue(place);
    q.receive({ ...room, fingerprint: "other" });
    expect(q.room.fingerprint).toBe("first");
    q.dispose();
  });
  it("optimistic transforms leave confirmed room immutable", () => {
    const next = optimistic(room, place, "temporary");
    expect(next.placements).toHaveLength(1);
    expect(room.placements).toEqual([]);
  });
});
describe("house preview rules", () => {
  const content = fixtureContent();
  it("matches floor collision and bounds refusal sentences", () => {
    expect(canPlace(content, 8, 8, [], place.item_id, 0, 0, false)).toBeNull();
    expect(canPlace(content, 8, 8, [place], place.item_id, 0, 0, false)).toBe(
      "something is already in that spot",
    );
    expect(canPlace(content, 8, 8, [], place.item_id, -1, 0, false)).toBe(
      "it does not fit inside the room there",
    );
    expect(canPlace(content, 8, 8, [], place.item_id, 8, 8, false)).toBe(
      "it does not fit inside the room there",
    );
  });
  it("allows different wall planes but refuses overlapping same-wall pieces", () => {
    const p = { item_id: "pebblepane_window__sand", x: 0, y: 0, flipped: false };
    expect(canPlace(content, 8, 8, [p], p.item_id, 0, 0, false)).toBe("something already hangs there");
    expect(canPlace(content, 8, 8, [p], p.item_id, 0, 0, true)).toBeNull();
  });
});

it("reconciles moved placement ids across queued edits and selection callbacks", async () => {
  let complete!: (value: Snapshot) => void;
  const before = { ...room, placements: [{ ...place, id: "old" }] };
  const send = vi
    .fn()
    .mockImplementationOnce(
      () =>
        new Promise<Snapshot>((resolve) => {
          complete = resolve;
        }),
    )
    .mockResolvedValueOnce({
      fingerprint: "third",
      placements: [{ ...place, id: "new", x: 2, flipped: true }],
      undo: [],
    });
  const changed = vi.fn();
  const q = new RoomQueue(before, send, async () => before, changed);
  q.enqueue({ type: "move", id: "old", x: 2, y: 0, flipped: false });
  q.enqueue({ type: "flip", id: "old" });
  complete({ fingerprint: "second", placements: [{ ...place, id: "new", x: 2 }], undo: [] });
  await tick();
  expect(send).toHaveBeenNthCalledWith(2, "second", { type: "flip", id: "new" });
  expect(changed.mock.calls.some((call) => call[3]?.old === "new")).toBe(true);
});

it("stops unsent operations when leaving a room or switching plaza", async () => {
  let finish!: (value: Snapshot) => void;
  const send = vi.fn(
    () =>
      new Promise<Snapshot>((resolve) => {
        finish = resolve;
      }),
  );
  const changed = vi.fn();
  const q = new RoomQueue(room, send, async () => room, changed);
  q.enqueue(place);
  q.enqueue({ type: "clear" });
  q.dispose();
  const calls = changed.mock.calls.length;
  q.enqueue(place);
  finish({ fingerprint: "second", placements: [{ ...place, id: "123" }], undo: [] });
  await tick();
  expect(send).toHaveBeenCalledTimes(1);
  expect(changed).toHaveBeenCalledTimes(calls);
  expect(q.pending).toBe(false);
});

it("shares no queue identity between two plaza rooms", async () => {
  const { housePath } = await import("./house");
  expect(housePath({ kind: "plaza", guild: "123" }, "/rooms/0")).not.toBe(
    housePath({ kind: "plaza", guild: "456" }, "/rooms/0"),
  );
});

it("cursor motion only notifies preview subscribers when crossing a tile", async () => {
  const { RoomCursor } = await import("./houseEdit");
  const cursor = new RoomCursor();
  const listener = vi.fn();
  const unsubscribe = cursor.subscribe(listener);
  const initial = cursor.get();
  for (let i = 0; i < 120; i++) cursor.move(0, 0);
  expect(cursor.get()).toBe(initial);
  expect(listener).not.toHaveBeenCalled();
  cursor.move(3, 2);
  expect(listener).toHaveBeenCalledTimes(1);
  expect(cursor.get()).toEqual([3, 2]);
  unsubscribe();
  cursor.move(4, 2);
  expect(listener).toHaveBeenCalledTimes(1);
});

it("coalesces pointer previews to one frame while placement reads the latest tile", async () => {
  const { RoomCursor } = await import("./houseEdit");
  let frame!: () => void;
  const request = vi.fn((callback: () => void) => {
    frame = callback;
    return 1;
  });
  vi.stubGlobal("requestAnimationFrame", request);
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  try {
    const cursor = new RoomCursor();
    const listener = vi.fn();
    cursor.subscribe(listener);
    cursor.move(1, 2);
    cursor.move(3, 4);
    expect(request).toHaveBeenCalledTimes(1);
    expect(cursor.get()).toEqual([3, 4]);
    expect(listener).not.toHaveBeenCalled();
    frame();
    expect(cursor.getSnapshot()).toEqual([3, 4]);
    expect(listener).toHaveBeenCalledTimes(1);
    cursor.move(5, 6);
    cursor.dispose();
    expect(cancelAnimationFrame).toHaveBeenCalledWith(1);
  } finally {
    vi.unstubAllGlobals();
  }
});

it("uses Discord's Turkish names in room tabs and house upgrade offers", async () => {
  const { roomLabel, tierLabel } = await import("./house");
  const { setLocale } = await import("../../../i18n");
  await setLocale("tr");
  try {
    expect(roomLabel("bedroom", "Drowsyfold Bedroom")).toBe("Drowsyfold Yatak Odası");
    expect(tierLabel(2, "Beach Cottage")).toBe("Kumsal Kulübesi");
    expect(tierLabel(6, "Sunken Palace")).toBe("Batık Saray");
    expect(roomLabel("future-room", "Custom room")).toBe("Custom room");
  } finally {
    await setLocale("en");
  }
  expect(tierLabel(2, "Beach Cottage")).toBe("Beach Cottage");
});

it("carries server revisions and distinct intention IDs across queued edits", async () => {
  const send = vi
    .fn()
    .mockResolvedValueOnce({ fingerprint: "placed", revision: "10", placements: [{ ...place, id: "123" }] })
    .mockResolvedValueOnce({ fingerprint: "cleared", revision: "11", placements: [] });
  const q = new RoomQueue(
    { ...room, revision: "9" },
    send,
    async () => room,
    () => {},
  );
  q.enqueue(place);
  q.enqueue({ type: "clear" });
  await tick();
  expect(send.mock.calls.map((c) => c[2])).toEqual(["9", "10"]);
  expect(send.mock.calls[0]![3]).toMatch(/^[0-9a-f-]{36}$/);
  expect(send.mock.calls[1]![3]).not.toBe(send.mock.calls[0]![3]);
  expect(q.room.revision).toBe("11");
});

it("retries a lost response with exactly the same idempotent request", async () => {
  const fetch = vi
    .fn()
    .mockRejectedValueOnce(new TypeError("connection lost"))
    .mockResolvedValueOnce(
      new Response(JSON.stringify({ fingerprint: "placed", revision: "10", placements: [] })),
    );
  vi.stubGlobal("fetch", fetch);
  try {
    await sendOperation({ kind: "own" }, 0, "first", place, "9", "intention-1");
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(fetch.mock.calls[0]).toEqual(fetch.mock.calls[1]);
    expect(JSON.parse(fetch.mock.calls[0]![1].body)).toMatchObject({
      request_id: "intention-1",
      expected_revision: "9",
    });
  } finally {
    vi.unstubAllGlobals();
  }
});

it("does not retry a server conflict or a legacy mutation without a receipt", async () => {
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(
      new Response(JSON.stringify({ error: { code: "room_changed", message: "changed" } }), { status: 409 }),
    );
  vi.stubGlobal("fetch", fetch);
  try {
    await expect(sendOperation({ kind: "own" }, 0, "first", place, "9")).rejects.toBeInstanceOf(ApiError);
    expect(fetch).toHaveBeenCalledTimes(1);
    fetch.mockRejectedValueOnce(new TypeError("connection lost"));
    await expect(sendOperation({ kind: "own" }, 0, "first", place)).rejects.toBeInstanceOf(TypeError);
    expect(fetch).toHaveBeenCalledTimes(2);
  } finally {
    vi.unstubAllGlobals();
  }
});
