import { createContext, useContext, type Dispatch, type SetStateAction } from "react";
import type { GuildInfo } from "../../../lib/api";
import type {
  ApplicationForm,
  CommunityEngagementData,
  Counting,
  MailSettings,
  QotdSettings,
} from "../api/communityEngagement";
import type { engagementSummary } from "./engagementSummary";

type State<T> = Dispatch<SetStateAction<T>>;

interface EngagementState {
  data: CommunityEngagementData;
  info?: GuildInfo;
  guild: string;
  qotd: QotdSettings;
  setQotd: State<QotdSettings>;
  counting: Counting;
  setCounting: State<Counting>;
  mail: MailSettings;
  setMail: State<MailSettings>;
  form: ApplicationForm | null;
  setForm: State<ApplicationForm | null>;
  postChannel: string | null;
  setPostChannel: State<string | null>;
  busy: boolean;
  write: (path: string, method: "PUT" | "POST", body?: unknown) => Promise<boolean>;
  summary: ReturnType<typeof engagementSummary>;
}

export const EngagementContext = createContext<EngagementState | null>(null);

export function useEngagementContext() {
  const value = useContext(EngagementContext);
  if (!value) throw new Error("Community settings context is missing");
  return value;
}
