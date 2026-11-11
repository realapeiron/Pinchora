import type { GuildInfo } from "../../../lib/api";
import type { Limits, PanelDraft, Question, Topic } from "../api/tickets";

export interface TabProps {
  draft: PanelDraft;
  set: <K extends keyof PanelDraft>(field: K, value: PanelDraft[K]) => void;
  limits: Limits;
  info?: GuildInfo;
  errors: Record<string, string>;
  base: string;
  most: number;
  updateTopic: (n: number, patch: Partial<Topic>) => void;
  updateQuestion: (n: number, patch: Partial<Question>) => void;
}
