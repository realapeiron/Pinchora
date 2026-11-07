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
  "Take over here": "Burada devral",
  Room: "Oda",
  "Loading your character…": "Karakterin yükleniyor…",
  "Tap a tile to explore": "Keşfetmek için bir karoya dokun",
  "Tap or click to walk or interact. Drag to pan; pinch or use + / − to zoom. Arrow keys move one tile. Home centres the camera.":
    "Yürümek veya etkileşmek için dokun. Kamerayı sürükle; yakınlaştırmak için iki parmak veya + / − kullan. Ok tuşları bir karo ilerletir. Home kamerayı ortalar.",
  "Playable room. Arrow keys move one tile. Home centres the camera.":
    "Oynanabilir oda. Ok tuşları bir karo ilerletir. Home kamerayı ortalar.",
  Connected: "Bağlandı",
  "Local preview": "Yerel önizleme",
  "Local preview — reconnecting": "Yerel önizleme — yeniden bağlanıyor",
  "This account is playing in another tab.": "Bu hesap başka bir sekmede oynuyor.",
  "You can no longer enter this house.": "Artık bu eve giremezsin.",
  "Sign in again to play together.": "Birlikte oynamak için yeniden giriş yap.",
  "This room is full.": "Bu oda dolu.",
  "Someone is already sitting there.": "Burada zaten biri oturuyor.",
  "Walk closer before using this furniture.": "Bu mobilyayı kullanmadan önce yaklaş.",
  "The layout changed. Try again.": "Odanın düzeni değişti. Yeniden dene.",
  "Please wait a moment.": "Lütfen biraz bekle.",
  "This furniture cannot be used.": "Bu mobilya kullanılamıyor.",
  "This room is unavailable.": "Bu oda şu anda kullanılamıyor.",
  "No free entry tile.": "Giriş için boş karo yok.",
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
const networkReasons: Record<string, string> = {
  controller_replaced: "This account is playing in another tab.",
  access_revoked: "You can no longer enter this house.",
  access_denied: "You can no longer enter this house.",
  session_expired: "Sign in again to play together.",
  authentication_failed: "Sign in again to play together.",
  room_full: "This room is full.",
  seat_occupied: "Someone is already sitting there.",
  too_far: "Walk closer before using this furniture.",
  stale_layout: "The layout changed. Try again.",
  stale_epoch: "The layout changed. Try again.",
  cooldown: "Please wait a moment.",
  rate_limited: "Please wait a moment.",
  unreachable: "That destination is unreachable.",
  not_usable: "This furniture cannot be used.",
  invalid_room: "This room is unavailable.",
  room_unavailable: "This room is unavailable.",
  no_free_entry: "No free entry tile.",
};
export function playerStatus(message: string): string {
  const preview = "Local preview — ";
  if (message.startsWith(preview) && networkReasons[message.slice(preview.length)])
    return t("Local preview") + " — " + t(networkReasons[message.slice(preview.length)]!);
  if (networkReasons[message]) return t(networkReasons[message]!);
  for (const prefix of ["Sitting on", "Using"])
    if (message.startsWith(`${prefix} `)) return `${t(prefix)} ${message.slice(prefix.length + 1)}`;
  return t(message);
}
