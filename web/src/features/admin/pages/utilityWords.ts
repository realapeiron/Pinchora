import { createContext } from "react";
import { catalogWords } from "../api/catalogWords";

export const UtilityWords = createContext({ t: catalogWords(), language: "en" });
