import type { AnchorHTMLAttributes } from "react";

import { INVITE_URL } from "./content";

/** The Add to Discord button. A plain link on purpose: `/invite` is the server's, not a page of the app. */
export function InviteButton({
  className,
  children = "Add to Discord",
  ...props
}: AnchorHTMLAttributes<HTMLAnchorElement>) {
  return (
    <a
      href={INVITE_URL}
      className={["pixel-button", "invite-button", className].filter(Boolean).join(" ")}
      {...props}
    >
      {children}
    </a>
  );
}
