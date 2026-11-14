import { t } from "../../../../../i18n";

export function petStatus(value: string) {
  switch (value) {
    case "away":
      return t("Away on a trip");
    case "asleep":
      return t("Asleep");
    case "back":
      return t("Back with a souvenir");
    default:
      return t("Awake");
  }
}
export function needName(value: string) {
  switch (value) {
    case "hunger":
      return t("Hunger");
    case "fun":
      return t("Fun");
    case "energy":
      return t("Energy");
    default:
      return t("Hygiene");
  }
}
