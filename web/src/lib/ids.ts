/** Discord ids are digits; anything else in a URL is not a server, channel or role. */
export function isDiscordId(id: string): boolean {
  return /^\d{1,20}$/.test(id);
}
