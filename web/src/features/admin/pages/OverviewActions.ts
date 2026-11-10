import { createContext, useContext } from "react";

export const OverviewActions = createContext({ openPalette: () => {} });
export const useOverviewActions = () => useContext(OverviewActions);
