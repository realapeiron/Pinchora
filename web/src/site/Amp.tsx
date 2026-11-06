import { Fragment, type ReactNode } from "react";

/** `text` for the pixel font, which draws an ampersand as something else: every `&` is set in the body font. */
export function Amp({ text }: { text: string }): ReactNode {
  const parts = text.split("&");
  return parts.map((part, i) => (
    <Fragment key={i}>
      {i > 0 && <span className="amp">&amp;</span>}
      {part}
    </Fragment>
  ));
}
