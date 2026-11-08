import { createContext } from "react";

/** The visible page title, used to avoid repeating it as a panel heading. */
export const PageTitleContext = createContext<string | null>(null);
