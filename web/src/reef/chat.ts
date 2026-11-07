export interface ChatLine {
  message_id: string;
  sender_id: string;
  name: string;
  text: string;
  created_at: string;
  request_id?: string;
  delivery: "pending" | "delivered" | "failed";
  reason?: string;
}
/** Immutable UI snapshots; only a server message can mark a line delivered. */
export class ChatState {
  private lines: ChatLine[] = [];
  private listeners = new Set<() => void>();
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  snapshot = () => this.lines;
  private publish(lines: ChatLine[]) {
    this.lines = lines.slice(-100);
    for (const listener of this.listeners) listener();
  }
  pending(request: string, sender: string, text: string) {
    this.publish([
      ...this.lines,
      {
        message_id: `pending-${request}`,
        sender_id: sender,
        name: "",
        text,
        request_id: request,
        created_at: new Date().toISOString(),
        delivery: "pending",
      },
    ]);
  }
  accept(line: Omit<ChatLine, "delivery">) {
    if (this.lines.some((old) => old.message_id === line.message_id)) return;
    this.publish([
      ...this.lines.filter((old) => !line.request_id || old.request_id !== line.request_id),
      { ...line, delivery: "delivered" },
    ]);
  }
  reject(request: string, reason: string) {
    this.publish(
      this.lines.map((line) =>
        line.request_id === request && line.delivery === "pending"
          ? { ...line, delivery: "failed", reason }
          : line,
      ),
    );
  }
  disconnected() {
    this.publish(
      this.lines.map((line) =>
        line.delivery === "pending" ? { ...line, delivery: "failed", reason: "delivery_unknown" } : line,
      ),
    );
  }
  clear() {
    this.publish([]);
  }
}
