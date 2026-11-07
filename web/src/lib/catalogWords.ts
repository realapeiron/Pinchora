/** UI catalogs come from the server's t! catalog in the guild language. */
export function catalogWords(strings: Record<string, string> = {}, language = "en") {
  return (key: string, args: Record<string, string | number> = {}) =>
    (strings[key] ?? key).replace(/\{(\w+)\}/g, (match, name: string) =>
      typeof args[name] === "number"
        ? args[name].toLocaleString(language === "tr" ? "tr-TR" : "en-US")
        : String(args[name] ?? match),
    );
}
