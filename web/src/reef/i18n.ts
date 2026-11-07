// Match the web's English-key / browser-language pattern (xpAdminI18n, tempVoiceI18n).
const tr: Record<string, string> = {
  "Play in this room": "Bu odada oyna",
  "Play camera": "Oyun kamerası",
  "Zoom out": "Uzaklaştır",
  "Zoom in": "Yakınlaştır",
  "Centre on me": "Bana odaklan",
  Controls: "Erişilebilir kontroller",
  "Close controls": "Kontrolleri kapat",
  "Furniture and actions": "Mobilyalar ve eylemler",
  "Tile X": "Karo X",
  "Tile Y": "Karo Y",
  "Walk to tile": "Karoya yürü",
  "Walk to": "Yanına yürü",
  Sit: "Otur",
  Use: "Kullan",
  View: "Görünüm",
  Room: "Oda",
  "Loading your character…": "Karakterin yükleniyor…",
  "Tap a tile to explore": "Keşfetmek için bir karoya dokun",
  "Tap or click to walk or interact. Drag to pan; pinch or use + / − to zoom. Arrow keys move one tile. Home centres the camera.":
    "Yürümek veya etkileşmek için dokun. Kamerayı sürükle; yakınlaştırmak için iki parmak veya + / − kullan. Ok tuşları bir karo ilerletir. Home kamerayı ortalar.",
  "Playable room. Arrow keys move one tile. Home centres the camera.":
    "Oynanabilir oda. Ok tuşları bir karo ilerletir. Home kamerayı ortalar.",
  Ready: "Hazır",
  Walking: "Yürüyor",
  "That destination is unreachable.": "Bu hedefe ulaşılamıyor.",
  "Sitting on": "Oturuyor:",
  Using: "Kullanıyor:",
};
export function t(english: string): string {
  return typeof navigator !== "undefined" && navigator.language.toLowerCase().startsWith("tr")
    ? (tr[english] ?? english)
    : english;
}
export function playerStatus(message: string): string {
  for (const prefix of ["Sitting on", "Using"])
    if (message.startsWith(`${prefix} `)) return `${t(prefix)} ${message.slice(prefix.length + 1)}`;
  return t(message);
}
