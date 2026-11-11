import type { GuildInfo } from "../../../lib/api";
export function channelName(info: GuildInfo | undefined, id: string): string {
  return `#${info?.channels.find((c) => c.id === id)?.name ?? id}`;
}
