import type { AnchorHTMLAttributes, ButtonHTMLAttributes } from "react";
import { Link } from "react-router";

type Variant = "primary" | "secondary" | "ghost";

function classes(variant: Variant, extra?: string) {
  return ["pixel-button", `pixel-button-${variant}`, extra].filter(Boolean).join(" ");
}

export function PixelButton({
  variant = "primary",
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return <button type="button" className={classes(variant, className)} {...props} />;
}

/** A button-looking link: in-app paths go through the router, others are plain links. */
export function PixelLink({
  variant = "primary",
  className,
  href,
  ...props
}: AnchorHTMLAttributes<HTMLAnchorElement> & { variant?: Variant; href: string }) {
  if (href.startsWith("/") && !href.startsWith("/auth/")) {
    return <Link to={href} className={classes(variant, className)} {...props} />;
  }
  return <a href={href} className={classes(variant, className)} {...props} />;
}
