import { Fragment, type ReactNode } from "react";

/**
 * The little Discord markdown the bot's sentences and messages use: **bold**, *italic* and `code`.
 * Everything else is shown as written (React escapes it).
 */
export function RichText({ text }: { text: string }) {
  return <>{render(text)}</>;
}

const TOKEN = /(\*\*[^*]+\*\*|`[^`]+`|\*[^*\s][^*]*\*)/g;

function render(text: string): ReactNode[] {
  return text.split(TOKEN).map((part, i) => {
    if (part.startsWith("**") && part.endsWith("**") && part.length > 4) {
      return <strong key={i}>{part.slice(2, -2)}</strong>;
    }
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2) {
      return <code key={i}>{part.slice(1, -1)}</code>;
    }
    if (part.startsWith("*") && part.endsWith("*") && part.length > 2) {
      return <em key={i}>{part.slice(1, -1)}</em>;
    }
    return <Fragment key={i}>{part}</Fragment>;
  });
}
