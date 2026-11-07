import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import { ReefNet, retryDelay, type NetPlayer } from "./net";
vi.mock("../lib/api", () => ({ api: vi.fn() }));

class Socket {
  static OPEN = 1;
  static instances: Socket[] = [];
  readyState = 1;
  sent: Record<string, unknown>[] = [];
  url: URL;
  onopen?: () => void;
  onmessage?: (event: { data: string }) => void;
  onclose?: (event: { reason: string }) => void;
  onerror?: () => void;
  constructor(url: URL) {
    this.url = url;
    Socket.instances.push(this);
  }
  send(raw: string) {
    this.sent.push(JSON.parse(raw));
  }
  close() {
    this.readyState = 3;
    this.onclose?.({ reason: "" });
  }
  message(body: object, sequence: number, epoch = "room-a") {
    this.onmessage?.({
      data: JSON.stringify({ protocol_version: 1, request_id: "", room_epoch: epoch, sequence, ...body }),
    });
  }
}
const player = (id = "1", position: [number, number] = [0, 7]): NetPlayer => ({
  user_id: id,
  name: "<script>plain name</script>",
  character: "saved.colours",
  tile: position,
  position,
  direction: "SE",
  state: "idle",
  seat: null,
});
let net: ReefNet;
let status: ReturnType<typeof vi.fn<(status: string) => void>>;
let layout: ReturnType<typeof vi.fn<() => void>>;
async function connected() {
  await net.connect();
  const socket = Socket.instances.at(-1)!;
  socket.onopen?.();
  socket.message({ type: "authenticated", user_id: "1" }, 1, "");
  socket.message({ type: "snapshot", revision: "r1", players: [player(), player("2")] }, 2);
  return socket;
}
beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("WebSocket", Socket);
  vi.stubGlobal("location", { href: "https://reef.test/me/house?play=1" });
  vi.mocked(api).mockReset();
  vi.mocked(api).mockResolvedValue({ ticket: "one-use" });
  Socket.instances = [];
  status = vi.fn();
  layout = vi.fn();
  net = new ReefNet("house:1:0", status, layout);
});
afterEach(() => {
  net.dispose();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("Reef socket", () => {
  it("authenticates in a frame, joins, and sends only intentions", async () => {
    const socket = await connected();
    expect(socket.url.href).toBe("wss://reef.test/api/reef/ws");
    expect(socket.sent[0]).toMatchObject({
      type: "authenticate",
      ticket: "one-use",
      sequence: 1,
      room_epoch: "",
    });
    expect(socket.sent[1]).toMatchObject({ type: "join", room: "house:1:0", sequence: 2 });
    expect(net.ready).toBe(true);
    expect(net.players.size).toBe(2);
    net.move([3, 7]);
    expect(socket.sent.at(-1)).toMatchObject({ type: "move_to", tile: [3, 7], room_epoch: "room-a" });
    expect(socket.sent.at(-1)).not.toHaveProperty("position");
  });
  it("requests a snapshot on gaps, ignores stale events, and replaces epochs", async () => {
    const socket = await connected();
    socket.message({ type: "correction", players: [player("2", [4, 7])] }, 4);
    expect(socket.sent.at(-1)?.type).toBe("snapshot");
    expect(net.players.get("2")?.position).toEqual([0, 7]);
    socket.message({ type: "snapshot", revision: "r2", players: [player()] }, 5, "room-b");
    expect(net.epoch).toBe("room-b");
    expect(net.players.size).toBe(1);
    socket.message({ type: "presence", player: player("3") }, 6, "room-a");
    expect(net.players.size).toBe(1);
    socket.message({ type: "presence", player: player("4") }, 5, "room-b");
    expect(net.players.size).toBe(1);
  });
  it("pauses intentions during recovery and requests only one snapshot", async () => {
    const socket = await connected();
    socket.message({ type: "correction", players: [player("2", [4, 7])] }, 4);
    expect(net.ready).toBe(false);
    expect(net.move([1, 7])).toBe(false);
    expect(net.say("Wait for the snapshot")).toBe(false);
    socket.message({ type: "correction", players: [player("2", [5, 7])] }, 7);
    expect(socket.sent.filter((m) => m.type === "snapshot")).toHaveLength(1);
    socket.message({ type: "snapshot", revision: "r2", players: [player()] }, 8);
    expect(net.ready).toBe(true);
    await vi.advanceTimersByTimeAsync(3001);
    expect(Socket.instances).toHaveLength(1);
    expect(net.ready).toBe(true);
  });
  it("reconnects if the requested snapshot is lost despite ongoing traffic", async () => {
    const socket = await connected();
    socket.message({ type: "correction" }, 4);
    await vi.advanceTimersByTimeAsync(2000);
    socket.message({ type: "correction" }, 5);
    await vi.advanceTimersByTimeAsync(1601);
    expect(Socket.instances).toHaveLength(2);
    expect(net.ready).toBe(false);
    const next = Socket.instances[1]!;
    next.onopen?.();
    next.message({ type: "authenticated", user_id: "1" }, 1, "");
    expect(next.sent.at(-1)).toMatchObject({ type: "join", room: "house:1:0" });
    next.message({ type: "snapshot", revision: "r2", players: [player()] }, 2, "fresh");
    expect(net.ready).toBe(true);
    expect(net.epoch).toBe("fresh");
  });
  it("recovers when the first routed snapshot is missing", async () => {
    await net.connect();
    const socket = Socket.instances[0]!;
    socket.onopen?.();
    socket.message({ type: "authenticated", user_id: "1" }, 1, "");
    socket.message({ type: "presence", player: player("2") }, 3);
    expect(socket.sent.at(-1)?.type).toBe("snapshot");
    expect(net.ready).toBe(false);
    socket.message({ type: "snapshot", revision: "r1", players: [player()] }, 4);
    expect(net.ready).toBe(true);
  });
  it("interpolates server samples at visual frame times", async () => {
    let now = 0;
    vi.spyOn(performance, "now").mockImplementation(() => now);
    const socket = await connected();
    now = 100;
    socket.message({ type: "correction", updates: [["2", 0.3, 7, 0, 7, "SE", "walk"]] }, 3);
    expect(net.visual("2", 150)?.position[0]).toBeCloseTo(0.15);
    expect(net.visual("2", 1000)?.position[0]).toBeCloseTo(0.3);
  });
  it("uses furniture only after authoritative arrival with the current revision", async () => {
    const socket = await connected();
    net.move([1, 7], { id: "seat", tile: [1, 7] });
    expect(socket.sent.at(-1)?.type).toBe("move_to");
    socket.message({ type: "correction", players: [{ ...player("1", [1, 7]), state: "walk" }] }, 3);
    expect(socket.sent.at(-1)?.type).toBe("move_to");
    socket.message({ type: "correction", players: [player("1", [1, 7])] }, 4);
    expect(socket.sent.at(-1)).toMatchObject({ type: "use", placement_id: "seat", revision: "r1" });
    socket.message({ type: "correction", players: [player("1", [1, 7])] }, 5);
    expect(socket.sent.filter((m) => m.type === "use")).toHaveLength(1);
  });
  it("clears peers and reconnects with a new ticket", async () => {
    vi.mocked(api)
      .mockResolvedValueOnce({ ticket: "first-ticket" })
      .mockResolvedValueOnce({ ticket: "second-ticket" });
    const socket = await connected();
    expect(socket.sent[0]).toMatchObject({ type: "authenticate", ticket: "first-ticket" });
    socket.close();
    expect(net.network).toBe("offline");
    expect(net.ready).toBe(false);
    expect(net.players.size).toBe(0);
    await vi.advanceTimersByTimeAsync(601);
    expect(api).toHaveBeenCalledTimes(2);
    expect(Socket.instances).toHaveLength(2);
    Socket.instances[1]!.onopen?.();
    expect(Socket.instances[1]!.sent[0]).toMatchObject({ type: "authenticate", ticket: "second-ticket" });
  });
  it("does not fight a replacement controller", async () => {
    const socket = await connected();
    socket.onclose?.({ reason: "controller_replaced" });
    expect(net.network).toBe("replaced");
    await vi.advanceTimersByTimeAsync(60_000);
    expect(api).toHaveBeenCalledTimes(1);
    expect(net.ready).toBe(false);
    expect(status).toHaveBeenLastCalledWith("controller_replaced");
    vi.mocked(api).mockResolvedValueOnce({ ticket: "takeover-ticket" });
    net.takeOver();
    await Promise.resolve();
    Socket.instances[1]!.onopen?.();
    expect(Socket.instances[1]!.sent[0]).toMatchObject({ type: "authenticate", ticket: "takeover-ticket" });
  });
  it("keeps network state within the documented states", async () => {
    expect(net.network).toBe("preview");
    const socket = await connected();
    expect(net.network).toBe("connected");
    socket.close();
    expect(net.network).toBe("offline");
    await vi.advanceTimersByTimeAsync(601);
    expect(net.network).toBe("connecting");
    expect(["preview", "connecting", "connected", "replaced", "offline"]).toContain(net.network);
  });
  it("refreshes layout and suppresses further intentions until a new snapshot", async () => {
    const socket = await connected();
    socket.message({ type: "layout_changed", revision: "r2" }, 3);
    expect(layout).toHaveBeenCalledTimes(1);
    expect(net.move([1, 7])).toBe(false);
  });
  it("expires silent connections and stops retries on disposal", async () => {
    await connected();
    await vi.advanceTimersByTimeAsync(45_001);
    expect(net.ready).toBe(false);
    net.dispose();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(api).toHaveBeenCalledTimes(1);
  });
  it("answers heartbeat and bounds backoff", async () => {
    const socket = await connected();
    socket.message({ type: "ping" }, 3, "");
    expect(socket.sent.at(-1)?.type).toBe("ping");
    expect(retryDelay(0, 0.5)).toBe(500);
    expect(retryDelay(99, 0.5)).toBe(30_000);
  });
  it("keeps rejected private rooms in local preview", async () => {
    const socket = await connected();
    socket.message({ type: "error", reason: "access_denied" }, 3);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(net.ready).toBe(false);
    expect(Socket.instances).toHaveLength(1);
  });
});
