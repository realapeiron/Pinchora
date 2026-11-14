import { t as deferredText } from "../../../../i18n/dashboard";
import { type EmbedSpec } from "../../../../lib/api";
import { blankEmbed } from "../../../../lib/tags";
import { type Component, type Kit } from "../../api/messageStudio";

export interface Saved {
  key: string;
  name: string;
  payload: Kit;
  version: number;
}
export interface Revision {
  version: number;
  created_at: string;
  payload: Kit;
}
export interface Rendered {
  content: string;
  embeds: EmbedSpec[];
  components: Component[];
  allowed_mentions: string;
}
export const empty = (): Kit => ({
  name: "",
  content: "",
  variants: [],
  embeds: [],
  components: [],
  allow_mass_mentions: false,
});
export const slug = (name: string) =>
  name
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40);
export const starter = (kind: "welcome" | "announcement" | "faq"): Kit => {
  if (kind === "welcome") {
    return {
      ...empty(),
      name: deferredText("Welcome kit"),
      content: deferredText("{{if returning}}Welcome back, {user}!{{else}}Welcome, {user}!{{end}}"),
      variants: [
        deferredText("{{if returning}}Welcome back, {user}!{{else}}Welcome, {user}!{{end}}"),
        deferredText(
          "{{if returning}}Glad to see you again, {user}!{{else}}Glad you're here, {user}!{{end}}",
        ),
      ],
    };
  }
  if (kind === "announcement") {
    return {
      ...empty(),
      name: deferredText("Announcement kit"),
      content: deferredText("News from {server}"),
      embeds: [
        {
          ...blankEmbed(),
          title: deferredText("Announcement title"),
          description: deferredText("Write your announcement here."),
        },
      ],
    };
  }
  return {
    ...empty(),
    name: deferredText("FAQ kit"),
    content: deferredText("Need help? Use the button below."),
    components: [
      {
        type: "button",
        label: deferredText("Get help"),
        url: null,
        response: deferredText("Ask a moderator for help."),
      },
    ],
  };
};
