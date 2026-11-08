/** A unique channel per test guild, outside the small IDs used by the legacy mock fixtures. */
export function countingChannelId(guild: string): string {
  return (BigInt(guild) ^ (1n << 62n)).toString();
}
