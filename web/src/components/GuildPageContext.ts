import { createContext, type ReactNode } from "react";

export const GuildPageContext = createContext<{
  group: string;
  icon: string;
  master?: ReactNode;
  offNote?: string;
} | null>(null);
