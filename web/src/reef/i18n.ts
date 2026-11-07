import { translate } from "../i18n";

export function t(english: string, values: Record<string, string | number> = {}): string {
  if (english === "no_action") english = "No action";
  if (english === "personal_info") english = "Personal information";
  if (english === "revoke") english = "Lift sanctions in this scope";
  return translate(english, values);
}

const networkReasons: Record<string, string> = {
  already_authenticated: "This request could not be completed.",
  invalid_command: "This request could not be completed.",
  invalid_envelope: "This request could not be completed.",
  invalid_message: "This request could not be completed.",
  not_joined: "This room is unavailable.",
  presence_expired: "Sign in again to play together.",
  room_busy: "Please wait a moment.",
  eligibility_required: "Confirm the chat age requirement and rules first.",
  chat_muted: "Chat is muted in this room.",
  chat_filtered: "Your message was not sent. Please remove unsafe language, links or personal information.",
  chat_length: "Messages must contain 1\u2013200 characters.",
  delivery_unknown: "Delivery could not be confirmed.",
  sanctioned: "Your session ended because of a moderation action.",
  logged_out: "You signed out.",
  consent_revoked: "Chat consent was withdrawn.",
  safety_unavailable: "This request could not be completed.",
  invalid_safety_request: "Check the safety form and try again.",

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
