import { ChatState, type ChatLine } from "./chat";
import { api } from "../lib/api";
import type { Direction, Point } from "./manifest";
import type { Target } from "./navigation";
import type { RoomJson } from "../iso/scene";

export interface NetPlayer {
  user_id: string;
  name: string;
  character: string;
  tile: Point;
  position: Point;
  direction: Direction;
  state: "idle" | "walk" | "sit" | "use";
  seat: Omit<Target, "label"> | null;
}
export type NetworkState = "preview" | "connecting" | "connected" | "replaced" | "offline";
interface Sample {
  at: number;
  player: NetPlayer;
}
interface ServerMessage {
  type: string;
  protocol_version: number;
  request_id: string;
  room_epoch: string;
  sequence: number;
  user_id?: string;
  revision?: string;
  players?: NetPlayer[];
  updates?: [string, number, number, number, number, Direction, NetPlayer["state"]][];
  player?: NetPlayer;
  action?: string;
  reason?: string;
  layout?: RoomJson;
  message_id?: string;
  sender_id?: string;
  name?: string;
  text?: string;
  created_at?: string;
}
export const retryDelay = (attempt: number, random = Math.random()) =>
  Math.min(30_000, 500 * 2 ** Math.min(attempt, 6)) * (0.8 + random * 0.4);

const layoutKey = (room: RoomJson) =>
  JSON.stringify([
    room.width,
    room.depth,
    room.wallpaper,
    room.floor,
    [...room.placements]
      .sort((a, b) => a.id.localeCompare(b.id))
      .map((p) => [p.id, p.item_id, p.x, p.y, p.flipped]),
  ]);

/** One room subscription. Server positions drive the roster; credentials never enter a URL. */
export class ReefNet {
  readonly chat = new ChatState();
  ready = false;
  network: NetworkState = "preview";
  userId = "";
  revision = "";
  epoch = "";
  readonly players = new Map<string, NetPlayer>();
  private samples = new Map<string, Sample[]>();
  private socket: WebSocket | null = null;
  private retry: ReturnType<typeof setTimeout> | undefined;
  private deadline: ReturnType<typeof setTimeout> | undefined;
  private disposed = false;
  private attempt = 0;
  private sequence = 0;
  private received = 0;
  private recovering = false;
  private recoveryDeadline: ReturnType<typeof setTimeout> | undefined;
  private generation = 0;
  private pending: { id: string; tile: Point } | null = null;
  private room: string;
  private initialLayout?: RoomJson;
  private changed: (status: string) => void;
  private layoutChanged: () => void;

  constructor(
    room: string,
    changed: (status: string) => void,
    layoutChanged: () => void,
    initialLayout?: RoomJson,
  ) {
    this.room = room;
    this.initialLayout = initialLayout;
    this.changed = changed;
    this.layoutChanged = layoutChanged;
  }
  async connect() {
    if (this.disposed) return;
    clearTimeout(this.retry);
    const generation = ++this.generation;
    this.network = "connecting";
    this.changed("Local preview — reconnecting");
    try {
      const { ticket } = await api<{ ticket: string }>("/api/reef/socket-ticket", "POST");
      if (this.disposed || generation !== this.generation) return;
      const url = new URL("/api/reef/ws", location.href);
      url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
      const socket = new WebSocket(url);
      this.socket = socket;
      this.sequence = 0;
      this.received = 0;
      this.epoch = "";
      this.recovering = false;
      const watchdog = (ms: number) => {
        clearTimeout(this.deadline);
        this.deadline = setTimeout(() => socket.close(), ms);
      };
      watchdog(8000);
      socket.onopen = () => this.send({ type: "authenticate", ticket });
      socket.onmessage = ({ data }) => {
        if (this.disposed || socket !== this.socket) return;
        watchdog(45_000);
        try {
          this.receive(JSON.parse(String(data)) as ServerMessage);
        } catch {
          socket.close();
        }
      };
      socket.onclose = ({ reason }) => {
        if (socket !== this.socket) return;
        clearTimeout(this.deadline);
        this.socket = null;
        this.reset();
        const terminal = [
          "controller_replaced",
          "access_revoked",
          "session_expired",
          "authentication_failed",
          "logged_out",
          "consent_revoked",
          "sanctioned",
        ].includes(reason);
        this.network = reason === "controller_replaced" ? "replaced" : terminal ? "preview" : "offline";
        this.changed(terminal ? reason : "Local preview — reconnecting");
        if (!this.disposed && !terminal) this.schedule();
      };
      socket.onerror = () => socket.close();
    } catch {
      if (!this.disposed && generation === this.generation) {
        this.network = "offline";
        this.changed("Local preview — reconnecting");
        this.schedule();
      }
    }
  }
  takeOver() {
    if (this.network === "replaced" && !this.disposed) void this.connect();
  }
  private schedule() {
    clearTimeout(this.retry);
    this.retry = setTimeout(() => void this.connect(), retryDelay(this.attempt++));
  }
  private reset() {
    clearTimeout(this.recoveryDeadline);
    this.chat.disconnected();
    this.ready = false;
    this.pending = null;
    this.players.clear();
    this.samples.clear();
  }
  send(body: Record<string, unknown>): boolean {
    if (this.socket?.readyState !== WebSocket.OPEN || this.disposed) return false;
    this.socket.send(
      JSON.stringify({
        ...body,
        protocol_version: 1,
        request_id: String(++this.sequence),
        room_epoch: this.epoch,
        sequence: this.sequence,
      }),
    );
    return true;
  }
  say(text: string) {
    if (!this.ready) return false;
    const request = String(this.sequence + 1);
    this.chat.pending(request, this.userId, text);
    if (!this.send({ type: "chat", text })) this.chat.reject(request, "delivery_unknown");
    return true;
  }
  move(tile: Point, seat?: { id: string; tile: Point }) {
    if (!this.ready) return false;
    this.pending = seat ?? null;
    this.changed("Walking");
    return this.send({ type: "move_to", tile });
  }
  private record(player: NetPlayer, now: number) {
    this.players.set(player.user_id, player);
    const previous = this.samples.get(player.user_id) ?? [];
    this.samples.set(player.user_id, [...previous, { at: now, player }].slice(-4));
  }
  private receive(message: ServerMessage) {
    if (message.protocol_version !== 1 || !Number.isSafeInteger(message.sequence))
      throw new Error("protocol");
    if (message.sequence <= this.received) return;
    const gap = this.received > 0 && message.sequence !== this.received + 1;
    this.received = message.sequence;
    // A missing event may have revoked chat visibility. Snapshots do not replay policy
    // notifications, and a policy event must take effect even while recovering a gap.
    if (gap || message.type === "policy_changed") this.chat.clear();
    if (message.type === "ping") this.send({ type: "ping" });
    if ((gap || this.recovering) && message.type !== "snapshot" && (this.epoch || this.userId)) {
      if (!this.recovering) {
        this.send({ type: "snapshot" });
        this.recoveryDeadline = setTimeout(() => this.socket?.close(), 3000);
      }
      this.ready = false;
      this.pending = null;
      this.recovering = true;
      return;
    }
    if (message.type === "authenticated") {
      this.userId = message.user_id!;
      this.send({ type: "join", room: this.room });
      return;
    }
    if (message.type === "policy_changed") return;
    if (message.type === "chat" && message.room_epoch === this.epoch) {
      this.chat.accept(message as unknown as Omit<ChatLine, "delivery">);
      return;
    }
    if (message.type === "error") {
      this.chat.reject(message.request_id, message.reason ?? "chat_filtered");
      this.pending = null;
      this.changed(message.reason ?? "Request rejected");
      if (["access_denied", "room_full", "no_free_entry", "invalid_room"].includes(message.reason ?? "")) {
        this.dispose();
        this.changed("Local preview — " + message.reason);
      }
      return;
    }
    const now = performance.now();
    if (message.type === "snapshot") {
      if (
        this.initialLayout &&
        message.layout &&
        layoutKey(this.initialLayout) !== layoutKey(message.layout)
      ) {
        this.ready = false;
        this.layoutChanged();
        return;
      }
      this.epoch = message.room_epoch;
      this.revision = message.revision!;
      this.reset();
      this.recovering = false;
      for (const player of message.players ?? []) this.record(player, now);
      this.ready = true;
      this.network = "connected";
      this.attempt = 0;
      this.changed("Connected");
    } else if (message.room_epoch === this.epoch) {
      if (message.type === "layout_changed") {
        this.pending = null;
        this.ready = false;
        this.layoutChanged();
        return;
      }
      if (message.type === "presence" && message.action === "leave") {
        this.players.delete(message.user_id!);
        this.samples.delete(message.user_id!);
      }
      if (message.player) this.record(message.player, now);
      for (const player of message.players ?? []) this.record(player, now);
      for (const [id, x, y, tx, ty, direction, state] of message.updates ?? []) {
        const old = this.players.get(id);
        if (old) this.record({ ...old, position: [x, y], tile: [tx, ty], direction, state }, now);
      }
    }
    const me = this.players.get(this.userId);
    if (this.pending && me?.state === "idle" && me.tile.every((n, i) => n === this.pending!.tile[i])) {
      this.send({ type: "use", placement_id: this.pending.id, revision: this.revision });
      this.pending = null;
    }
  }
  /** 100 ms interpolation buffer, evaluated by requestAnimationFrame, without React state. */
  visual(id: string, now = performance.now()): NetPlayer | undefined {
    const samples = this.samples.get(id);
    if (!samples?.length) return undefined;
    const at = now - 100;
    const next = samples.find((s) => s.at >= at) ?? samples.at(-1)!;
    const prev = [...samples].reverse().find((s) => s.at <= at) ?? samples[0]!;
    if (prev === next || prev.player.state === "sit" || next.player.state === "sit") return next.player;
    const t = Math.max(0, Math.min(1, (at - prev.at) / (next.at - prev.at || 1)));
    return {
      ...next.player,
      position: [
        prev.player.position[0] + (next.player.position[0] - prev.player.position[0]) * t,
        prev.player.position[1] + (next.player.position[1] - prev.player.position[1]) * t,
      ],
    };
  }
  dispose() {
    this.disposed = true;
    this.generation++;
    clearTimeout(this.retry);
    clearTimeout(this.deadline);
    this.reset();
    const socket = this.socket;
    this.socket = null;
    socket?.close();
  }
}
