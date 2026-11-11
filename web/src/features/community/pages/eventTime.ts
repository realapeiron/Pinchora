export const eventHour = (iso: string, tz: string, language: string) =>
  new Intl.DateTimeFormat(language, {
    timeZone: tz,
    hour: "numeric",
    minute: "2-digit",
    timeZoneName: "short",
  }).format(new Date(iso));
