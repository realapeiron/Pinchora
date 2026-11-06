import type { ElementType, ReactNode } from "react";

import { useInView } from "./motion";

/** Fades its children in the first time they scroll into view (CSS turns this off for reduced motion). */
export function Reveal({
  as: Tag = "div",
  className,
  children,
  ...rest
}: {
  as?: ElementType;
  className?: string;
  children: ReactNode;
  id?: string;
}) {
  const [ref, seen] = useInView<HTMLElement>();
  return (
    <Tag ref={ref} className={["reveal", seen ? "is-in" : "", className].filter(Boolean).join(" ")} {...rest}>
      {children}
    </Tag>
  );
}
