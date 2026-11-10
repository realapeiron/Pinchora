import type { AnchorHTMLAttributes, ButtonHTMLAttributes } from "react";
import { Link } from "react-router";

type Variant = "primary" | "secondary" | "ghost" | "quiet" | "danger";

function classes(variant: Variant, extra?: string, small?: boolean) {
  return ["pixel-button", `pixel-button-${variant}`, small && "pixel-button-small", extra]
    .filter(Boolean)
    .join(" ");
}

export function PixelButton({
  variant = "primary",
  small = false,
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; small?: boolean }) {
  return <button type="button" className={classes(variant, className, small)} {...props} />;
}

/** A button-looking link: in-app paths go through the router, others are plain links. */
export function PixelLink({
  variant = "primary",
  small = false,
  className,
  href,
  children,
  ...props
}: AnchorHTMLAttributes<HTMLAnchorElement> & { variant?: Variant; href: string; small?: boolean }) {
  if (href.startsWith("/") && !href.startsWith("/auth/")) {
    return (
      <Link to={href} className={classes(variant, className, small)} {...props}>
        {children}
      </Link>
    );
  }
  return (
    <a href={href} className={classes(variant, className, small)} {...props}>
      {children}
    </a>
  );
}
