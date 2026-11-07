import { Fragment, type ReactNode } from "react";

import { richParts } from "../lib/text";
import { translate } from "../i18n";

/**
 * The little Discord markdown the bot's sentences and messages use: **bold**, *italic* and `code`, and
 * backslash escapes, which Discord hides (the bot escapes what members type). Everything else is shown as
 * written (React escapes it).
 */
export function RichText({ text }: { text: string }) {
  return <>{render(translate(text))}</>;
}

function render(text: string): ReactNode[] {
  return richParts(text).map((part, i) => {
    switch (part.kind) {
      case "bold":
        return <strong key={i}>{part.text}</strong>;
      case "code":
        return <code key={i}>{part.text}</code>;
      case "italic":
        return <em key={i}>{part.text}</em>;
      default:
        return <Fragment key={i}>{part.text}</Fragment>;
    }
  });
}
