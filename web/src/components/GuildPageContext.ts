import { createContext } from "react";

export const GuildPageContext = createContext<{ group: string; icon: string } | null>(null);
