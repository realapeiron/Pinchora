import { translate as t } from "../i18n";
import { ApiError } from "./api";
import type { EmbedSpec } from "./api";

export type Component =
  | { type: "button"; label: string; url: string | null; response: string | null }
  | { type: "select"; placeholder: string; options: { label: string; value: string; response: string }[] };
export interface Kit {
  name: string;
  content: string;
  variants: string[];
  embeds: EmbedSpec[];
  components: Component[];
  allow_mass_mentions: boolean;
}

/** Untrusted imports enter editor state only after the server returns its canonical schema. */
export async function importKit(text: string, validate: (value: unknown) => Promise<Kit>): Promise<Kit> {
  if (new TextEncoder().encode(text).length > 32 * 1024) throw new Error("oversized");
  const kit = await validate(JSON.parse(text));
  // Approval cannot arrive from a downloaded file; an administrator must opt in again.
  return { ...kit, allow_mass_mentions: false };
}

export function componentRows<T extends { type: "button" | "select" }>(components: T[]): T[][] {
  const rows: T[][] = [];
  for (const component of components) {
    const last = rows.at(-1);
    if (component.type === "button" && last?.[0]?.type === "button" && last.length < 5) {
      last.push(component);
    } else {
      rows.push([component]);
    }
  }
  return rows;
}

/** Localized validation errors; unexpected server details use the translated action fallback. */
export function studioError(error: unknown, fallback: string): string {
  if (!(error instanceof ApiError)) return fallback;
  const messages: Record<string, string> = {
    "Use balanced, non-nested if/else conditions in at most 2,000 characters.": t(
      "Use balanced, non-nested if/else conditions in at most 2,000 characters.",
    ),
    "Use 1 to 60 characters for the kit name.": t("Use 1 to 60 characters for the kit name."),
    "Content and each of at most 10 random variants must fit Discord's 2,000 characters.": t(
      "Content and each of at most 10 random variants must fit Discord's 2,000 characters.",
    ),
    "A message has at most 10 embeds.": t("A message has at most 10 embeds."),
    "All embeds together have at most 6,000 characters.": t(
      "All embeds together have at most 6,000 characters.",
    ),
    "Use a valid HTTPS URL without credentials.": t("Use a valid HTTPS URL without credentials."),
    "Add content, a random variant, or an embed.": t("Add content, a random variant, or an embed."),
    "A message has at most 5 component rows.": t("A message has at most 5 component rows."),
    "A button needs a label and exactly one HTTPS link or reply.": t(
      "A button needs a label and exactly one HTTPS link or reply.",
    ),
    "Use an HTTPS link under 512 characters or a reply under 2,000 characters.": t(
      "Use an HTTPS link under 512 characters or a reply under 2,000 characters.",
    ),
    "A select needs 1 to 25 labeled choices with short values and replies.": t(
      "A select needs 1 to 25 labeled choices with short values and replies.",
    ),
    "A saved kit is at most 32 KiB of JSON.": t("A saved kit is at most 32 KiB of JSON."),
    "Choose welcome, goodbye, boost or level up.": t("Choose welcome, goodbye, boost or level up."),
    "Save the kit before assigning it.": t("Save the kit before assigning it."),
    "Event text kits cannot have embeds or components.": t(
      "Event text kits cannot have embeds or components.",
    ),
    "Use a short lowercase kit key with letters, numbers, _ or -.": t(
      "Use a short lowercase kit key with letters, numbers, _ or -.",
    ),
    "A kit is at most 32 KiB of JSON.": t("A kit is at most 32 KiB of JSON."),
    "Check the kit JSON fields and types.": t("Check the kit JSON fields and types."),
    "A kit assigned to an event cannot have embeds or components.": t(
      "A kit assigned to an event cannot have embeds or components.",
    ),
    "Only a server administrator can allow everyone and role mentions.": t(
      "Only a server administrator can allow everyone and role mentions.",
    ),
    "A server can save 50 kits.": t("A server can save 50 kits."),
    "That kit version is no longer available.": t("That kit version is no longer available."),
    "Save this kit before posting it.": t("Save this kit before posting it."),
    "Enter a message ID.": t("Enter a message ID."),
    "Image alt text is at most 1024 characters.": t("Image alt text is at most 1024 characters."),
    "That saved version no longer exists.": t("That saved version no longer exists."),
    "Discord could not open your direct messages.": t("Discord could not open your direct messages."),
    "Check this kit's fields and Discord limits.": t("Check this kit's fields and Discord limits."),
  };
  return (
    messages[error.message] ??
    (error.status === 422 ? t("Check this kit's fields and Discord limits.") : fallback)
  );
}
